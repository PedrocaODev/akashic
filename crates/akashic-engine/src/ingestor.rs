use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;
use walkdir::WalkDir;

pub struct GitFileDiff {
    pub status: String,
    pub path: String,
}

pub fn resolve_commit_sha(repo_path: &Path, rev: &str) -> Result<String> {
    let output = Command::new("git")
        .arg("--no-optional-locks")
        .arg("-C")
        .arg(repo_path)
        .arg("rev-parse")
        .arg(rev)
        .output()
        .context("Failed to resolve commit sha")?;
    
    if !output.status.success() {
        anyhow::bail!("git rev-parse failed");
    }
    
    Ok(String::from_utf8(output.stdout)?.trim().to_string())
}

pub fn list_tree_files(repo_path: &Path, rev: &str) -> Result<Vec<String>> {
    let output = Command::new("git")
        .arg("--no-optional-locks")
        .arg("-C")
        .arg(repo_path)
        .arg("ls-tree")
        .arg("-r")
        .arg("--name-only")
        .arg(rev)
        .output()?;
        
    if !output.status.success() {
        anyhow::bail!("git ls-tree failed");
    }
    
    let stdout = String::from_utf8(output.stdout)?;
    Ok(stdout.lines().map(|s| s.to_string()).collect())
}

pub fn read_blob_at_commit(repo_path: &Path, rev: &str, file_path: &str) -> Result<String> {
    let output = Command::new("git")
        .arg("--no-optional-locks")
        .arg("-C")
        .arg(repo_path)
        .arg("show")
        .arg(format!("{}:{}", rev, file_path))
        .output()?;
        
    if !output.status.success() {
        anyhow::bail!("git show failed");
    }
    
    Ok(String::from_utf8(output.stdout)?)
}

pub fn diff_commits(repo_path: &Path, old_rev: &str, new_rev: &str) -> Result<Vec<GitFileDiff>> {
    let output = Command::new("git")
        .arg("--no-optional-locks")
        .arg("-C")
        .arg(repo_path)
        .arg("diff-tree")
        .arg("-r")
        .arg("--no-commit-id")
        .arg("--name-status")
        .arg("-M")
        .arg(old_rev)
        .arg(new_rev)
        .output()?;
        
    if !output.status.success() {
        anyhow::bail!("git diff-tree failed");
    }
    
    let stdout = String::from_utf8(output.stdout)?;
    let mut diffs = Vec::new();
    for line in stdout.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 2 {
            diffs.push(GitFileDiff {
                status: parts[0].to_string(),
                path: parts[parts.len() - 1].to_string(),
            });
        }
    }
    Ok(diffs)
}

pub fn walk_worktree_files(repo_path: &Path) -> Result<Vec<String>> {
    let mut files = Vec::new();
    for entry in WalkDir::new(repo_path) {
        let entry = entry?;
        if entry.file_type().is_file() {
            if let Ok(rel_path) = entry.path().strip_prefix(repo_path) {
                if let Some(path_str) = rel_path.to_str() {
                    files.push(path_str.to_string());
                }
            }
        }
    }
    Ok(files)
}
