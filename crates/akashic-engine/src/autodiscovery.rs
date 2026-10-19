use std::path::Path;

fn interpolate_vars(val: &str, vars: &std::collections::HashMap<String, String>) -> String {
    let mut res = val.to_string();
    for (k, v) in vars {
        res = res.replace(&format!("${{{}}}", k), v);
        res = res.replace(&format!("${}", k), v);
        res = res.replace(&format!("project.{}", k), v);
        res = res.replace(&format!("rootProject.ext.{}", k), v);
    }
    res
}

use std::fs;
use std::collections::{HashMap, HashSet};
use anyhow::{Result, Context};
use crate::store::{GraphStore, RepositoryRecord, PackageMapEntry};

pub fn scan_repository(repo_path: &Path) -> Result<(RepositoryRecord, Vec<PackageMapEntry>)> {
    let repo_name = repo_path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown_repo".to_string());
    
    // Pass 1: Locate Git & Gradle root, parse .git/config for Gerrit
    let gradle_root = repo_path.join("settings.gradle");
    let has_gradle = gradle_root.exists() || repo_path.join("settings.gradle.kts").exists();
    let has_git = repo_path.join(".git/config").exists();
    
    let mut gerrit_project = String::new();
    if has_git {
        if let Ok(config) = fs::read_to_string(repo_path.join(".git/config")) {
            for line in config.lines() {
                if line.contains("url =") && line.contains("gerrit") {
                    let parts: Vec<&str> = line.split("gerrit").collect();
                    if parts.len() > 1 {
                        let proj = parts[1].split(':').last().unwrap_or("").trim_end_matches(".git").trim();
                        gerrit_project = proj.to_string();
                    }
                    break;
                }
            }
        }
    }
    
    // Pass 2: Root variable & property interpolation
    let mut ext_vars = HashMap::new();
    if let Ok(props) = fs::read_to_string(repo_path.join("gradle.properties")) {
        for line in props.lines() {
            let line = line.trim();
            if !line.starts_with('#') {
                if let Some((k, v)) = line.split_once('=') {
                    ext_vars.insert(k.trim().to_string(), v.trim().to_string());
                }
            }
        }
    }
    
    // Pass 3 & 4 & 5: Build file inspection, Manifest/BP parsing, Source scanning
    let mut packages = Vec::new();
    let mut primary_app_id = String::new();
    
    let walker = walkdir::WalkDir::new(repo_path).into_iter();
    
    let mut package_prefixes = HashSet::new();
    
    for entry in walker.filter_map(|e| e.ok()) {
        if entry.file_type().is_file() {
            let name = entry.file_name().to_string_lossy();
            let path_str = entry.path().to_string_lossy().to_string();
            
            if name == "build.gradle" || name == "build.gradle.kts" {
                if let Ok(content) = fs::read_to_string(entry.path()) {
                    let mut found_namespace = String::new();
                    let mut found_app_id = String::new();
                    let mut found_suffix = String::new();
                    let mut flavors = Vec::new();
                    
                    for line in content.lines() {
                        let trimmed = line.trim();
                        if trimmed.starts_with("namespace ") || trimmed.starts_with("namespace=") {
                            let ns = trimmed.replace("namespace", "").replace("=", "").replace("'", "").replace("\"", "").trim().to_string();
                            found_namespace = interpolate_vars(&ns, &ext_vars);
                        } else if trimmed.starts_with("applicationId ") || trimmed.starts_with("applicationId=") {
                            let id = trimmed.replace("applicationId", "").replace("=", "").replace("'", "").replace("\"", "").trim().to_string();
                            found_app_id = interpolate_vars(&id, &ext_vars);
                        } else if trimmed.starts_with("applicationIdSuffix") {
                            let suffix = trimmed.replace("applicationIdSuffix", "").replace("=", "").replace("'", "").replace("\"", "").trim().to_string();
                            found_suffix = interpolate_vars(&suffix, &ext_vars);
                        } else if trimmed.contains("flavor") {
                            // simple mock for flavors
                            flavors.push("demo".to_string());
                        }
                    }
                    
                    if !found_app_id.is_empty() {
                        if primary_app_id.is_empty() {
                            primary_app_id = found_app_id.clone();
                        }
                        packages.push(PackageMapEntry {
                            package_id: found_app_id.clone(),
                            repo_name: repo_name.clone(),
                            source_type: "application_id".to_string(),
                            module_gradle_path: path_str.clone(),
                            role_priority: 1,
                            flavor_name: String::new(),
                            is_primary_app: true,
                        });
                        
                        if !found_suffix.is_empty() {
                            packages.push(PackageMapEntry {
                                package_id: format!("{}{}", found_app_id, found_suffix),
                                repo_name: repo_name.clone(),
                                source_type: "flavor_application_id".to_string(),
                                module_gradle_path: path_str.clone(),
                                role_priority: 3,
                                flavor_name: "flavor".to_string(),
                                is_primary_app: false,
                            });
                        }
                    } else if !found_namespace.is_empty() {
                        packages.push(PackageMapEntry {
                            package_id: found_namespace.clone(),
                            repo_name: repo_name.clone(),
                            source_type: "module_namespace".to_string(),
                            module_gradle_path: path_str.clone(),
                            role_priority: 5,
                            flavor_name: String::new(),
                            is_primary_app: false,
                        });
                    }
                }
            } else if name == "AndroidManifest.xml" {
                if let Ok(content) = fs::read_to_string(entry.path()) {
                    if let Some(start) = content.find("package=\"") {
                        if let Some(end) = content[start + 9..].find("\"") {
                            let pkg = &content[start + 9..start + 9 + end];
                            packages.push(PackageMapEntry {
                                package_id: pkg.to_string(),
                                repo_name: repo_name.clone(),
                                source_type: "manifest_package".to_string(),
                                module_gradle_path: path_str.clone(),
                                role_priority: 6,
                                flavor_name: String::new(),
                                is_primary_app: false,
                            });
                        }
                    }
                }
            } else if name == "Android.bp" {
                if let Ok(content) = fs::read_to_string(entry.path()) {
                    let is_app = content.contains("android_app");
                    if is_app {
                        for line in content.lines() {
                            let trimmed = line.trim();
                            if trimmed.starts_with("name:") {
                                let bp_name = trimmed.replace("name:", "").replace("\"", "").trim().trim_end_matches(',').to_string();
                                packages.push(PackageMapEntry {
                                    package_id: bp_name.clone(),
                                    repo_name: repo_name.clone(),
                                    source_type: "android_bp".to_string(),
                                    module_gradle_path: path_str.clone(),
                                    role_priority: 4,
                                    flavor_name: String::new(),
                                    is_primary_app: true,
                                });
                                break;
                            }
                        }
                    }
                }
            } else if name.ends_with(".java") || name.ends_with(".kt") {
                if let Ok(content) = fs::read_to_string(entry.path()) {
                    for line in content.lines() {
                        let trimmed = line.trim();
                        if trimmed.starts_with("package ") {
                            let pkg = trimmed.replace("package ", "").trim_end_matches(';').trim().to_string();
                            if !pkg.is_empty() {
                                package_prefixes.insert(pkg);
                            }
                            break;
                        }
                    }
                }
            }
        }
    }
    
    // Convert source prefixes
    for prefix in package_prefixes {
        packages.push(PackageMapEntry {
            package_id: prefix.clone(),
            repo_name: repo_name.clone(),
            source_type: "source_prefix".to_string(),
            module_gradle_path: repo_path.to_string_lossy().to_string(),
            role_priority: 7,
            flavor_name: String::new(),
            is_primary_app: false,
        });
    }
    
    if primary_app_id.is_empty() && !packages.is_empty() {
        primary_app_id = packages[0].package_id.clone();
    }
    
    let vault_partition = if primary_app_id.is_empty() {
        String::new()
    } else {
        format!("akashic/{}", primary_app_id)
    };

    let repo_record = RepositoryRecord {
        repo_name: repo_name.clone(),
        worktree_path: repo_path.to_string_lossy().to_string(),
        gradle_root_path: if has_gradle { repo_path.to_string_lossy().to_string() } else { String::new() },
        gerrit_project,
        primary_app_id,
        vault_partition,
        last_indexed_sha: "HEAD".to_string(),
        updated_at: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64,
    };
    
    Ok((repo_record, packages))
}

pub fn discover_and_index_repository(store: &GraphStore, repo_path: &Path) -> Result<(RepositoryRecord, Vec<PackageMapEntry>)> {
    let (repo, pkgs) = scan_repository(repo_path)?;
    store.upsert_repository(&repo)?;
    store.clear_package_map_by_repo(&repo.repo_name)?;
    for pkg in &pkgs {
        store.upsert_package_map(pkg)?;
    }
    Ok((repo, pkgs))
}
