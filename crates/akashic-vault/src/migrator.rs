use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use walkdir::WalkDir;

use crate::graph::VaultGraph;
use crate::parser::{parse_markdown_file, serialize_document};
use crate::VaultError;

/// Action category for planned file change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeAction {
    Modify,
    Create,
    Delete,
}

/// A planned modification to a file in the vault.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlannedFileChange {
    pub path: String,
    pub before_sha256: String,
    pub after_sha256: String,
    pub action: ChangeAction,
    pub diff: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_content: Option<String>,
}

/// Complete migration plan detailing proposed vault modifications.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MigrationPlan {
    pub plan_version: String,
    pub vault_root: String,
    pub changes: Vec<PlannedFileChange>,
}

/// Computes lowercase hex SHA-256 digest of bytes.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

/// Generates a unified diff string between original and modified text.
pub fn generate_unified_diff(path: &str, original: &str, modified: &str) -> String {
    let orig_lines: Vec<&str> = original.lines().collect();
    let mod_lines: Vec<&str> = modified.lines().collect();
    let from_file = format!("a/{}", path);
    let to_file = format!("b/{}", path);
    let diff = difflib::unified_diff(&orig_lines, &mod_lines, &from_file, &to_file, "", "", 3);
    let mut out = diff.join("\n");
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// Inspects a vault directory and generates an idempotent migration plan.
pub fn generate_plan(root: &Path) -> Result<MigrationPlan, VaultError> {
    if !root.exists() {
        return Err(VaultError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("Vault directory not found: {}", root.display()),
        )));
    }

    let mut changes = Vec::new();

    for entry_res in WalkDir::new(root) {
        let entry = match entry_res {
            Ok(e) => e,
            Err(_) => continue,
        };

        if entry.file_type().is_file() {
            let path = entry.path();
            if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                if ext.eq_ignore_ascii_case("md") || ext.eq_ignore_ascii_case("markdown") {
                    let rel_path = path
                        .strip_prefix(root)
                        .unwrap_or(path)
                        .to_string_lossy()
                        .replace('\\', "/");

                    // Skip files inside .akashic
                    if rel_path.starts_with(".akashic/") || rel_path.starts_with(".git/") {
                        continue;
                    }

                    let original_content = fs::read_to_string(path)?;
                    let before_sha = sha256_hex(original_content.as_bytes());

                    if let Ok(doc) = parse_markdown_file(root, path) {
                        if let Ok(serialized) = serialize_document(&doc.frontmatter, &doc.body) {
                            if serialized != original_content {
                                let after_sha = sha256_hex(serialized.as_bytes());
                                let diff = generate_unified_diff(&rel_path, &original_content, &serialized);

                                changes.push(PlannedFileChange {
                                    path: rel_path,
                                    before_sha256: before_sha,
                                    after_sha256: after_sha,
                                    action: ChangeAction::Modify,
                                    diff,
                                    new_content: Some(serialized),
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    changes.sort_by(|a, b| a.path.cmp(&b.path));

    Ok(MigrationPlan {
        plan_version: "0.2.0".to_string(),
        vault_root: root.display().to_string(),
        changes,
    })
}

/// Alias for `generate_plan`.
pub fn plan_vault(root: &Path) -> Result<MigrationPlan, VaultError> {
    generate_plan(root)
}

/// Previews the resulting graph if a migration plan were applied.
pub fn preview_plan(root: &Path, plan: &MigrationPlan) -> Result<VaultGraph, VaultError> {
    VaultGraph::from_dir_with_plan(root, plan)
}

/// Applies an approved migration plan to a vault with locking, hash verification, and backups.
pub fn apply_plan(root: &Path, plan: &MigrationPlan) -> Result<usize, VaultError> {
    apply_plan_with_validator(root, plan, |_| Ok(()))
}

/// Applies an approved migration plan to a vault with locking, hash verification, backups,
/// and a pre-apply validator gate run against the preview graph before disk mutations.
pub fn apply_plan_with_validator<F>(
    root: &Path,
    plan: &MigrationPlan,
    validator: F,
) -> Result<usize, VaultError>
where
    F: FnOnce(&VaultGraph) -> Result<(), VaultError>,
{
    if !root.exists() {
        return Err(VaultError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("Vault directory not found: {}", root.display()),
        )));
    }

    // Phase 1: Pre-image SHA-256 verification
    for change in &plan.changes {
        let target_path = root.join(&change.path);
        match change.action {
            ChangeAction::Create => {
                if target_path.exists() {
                    let current_bytes = fs::read(&target_path)?;
                    let current_sha = sha256_hex(&current_bytes);
                    if current_sha != change.after_sha256 {
                        return Err(VaultError::Migration(format!(
                            "Target file already exists for Create action: {}",
                            change.path
                        )));
                    }
                }
            }
            ChangeAction::Modify | ChangeAction::Delete => {
                if !target_path.exists() {
                    return Err(VaultError::Migration(format!(
                        "Target file not found: {}",
                        change.path
                    )));
                }

                let current_bytes = fs::read(&target_path)?;
                let current_sha = sha256_hex(&current_bytes);

                if current_sha != change.before_sha256 && current_sha != change.after_sha256 {
                    return Err(VaultError::Migration(format!(
                        "Hash mismatch for file '{}': expected source SHA-256 '{}', found '{}'. Vault was modified after plan generation.",
                        change.path, change.before_sha256, current_sha
                    )));
                }
            }
        }
    }

    // Phase 1.5: Pre-Apply Validation Gate
    let preview_graph = preview_plan(root, plan)?;
    validator(&preview_graph)?;

    if plan.changes.is_empty() {
        return Ok(0);
    }

    // Phase 2: Create backup snapshot under <root>/.akashic/backups/<timestamp>/
    let akashic_dir = root.join(".akashic");
    fs::create_dir_all(&akashic_dir)?;

    let lock_path = akashic_dir.join("vault.lock");
    let lock_file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(&lock_path)
        .map_err(|e| VaultError::Lock(format!("Failed to open lockfile: {}", e)))?;

    fs2::FileExt::lock_exclusive(&lock_file)
        .map_err(|e| VaultError::Lock(format!("Failed to acquire exclusive lock: {}", e)))?;

    let timestamp = Utc::now().format("%Y%m%d_%H%M%S").to_string();
    let backup_root = akashic_dir.join("backups").join(&timestamp);

    for change in &plan.changes {
        let target_path = root.join(&change.path);
        let backup_file = backup_root.join(&change.path);
        if let Some(parent) = backup_file.parent() {
            fs::create_dir_all(parent)?;
        }
        if target_path.exists() {
            fs::copy(&target_path, &backup_file)?;
        }
    }

    // Phase 3: Apply modifications atomically
    let mut files_modified = 0;

    for change in &plan.changes {
        let target_path = root.join(&change.path);
        match change.action {
            ChangeAction::Delete => {
                if target_path.exists() {
                    fs::remove_file(&target_path)?;
                    files_modified += 1;
                }
            }
            ChangeAction::Create | ChangeAction::Modify => {
                if target_path.exists() {
                    let current_bytes = fs::read(&target_path)?;
                    let current_sha = sha256_hex(&current_bytes);
                    if current_sha == change.after_sha256 {
                        // Already applied
                        continue;
                    }
                }

                if let Some(ref new_content) = change.new_content {
                    if let Some(parent) = target_path.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    let temp_path = target_path.with_extension(format!(
                        "tmp.{}",
                        Utc::now().timestamp_nanos_opt().unwrap_or(0)
                    ));
                    fs::write(&temp_path, new_content)?;
                    fs::rename(&temp_path, &target_path)?;
                    files_modified += 1;
                }
            }
        }
    }

    Ok(files_modified)
}
