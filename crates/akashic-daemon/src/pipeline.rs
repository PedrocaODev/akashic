use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use akashic_engine::autodiscovery::discover_and_index_repository;
use akashic_engine::reconciler::index_repository_at_rev;
use akashic_engine::store::{GraphStore, PackageMapEntry, RepositoryRecord};
use akashic_sanitize::CodeProfile;
use akashic_embedding::OfflineEmbeddingProvider;
use akashic_vault::{VaultGraph, VaultIndex};

use crate::debounce::CoalescedProjectBatch;
use crate::watermark::{WatermarkRecord, WatermarkStore};
use crate::DaemonError;

#[derive(Debug, Clone, Default)]
pub struct WorkspaceDiscoveryReport {
    pub discovered_repositories: Vec<RepositoryRecord>,
    pub package_mappings: Vec<PackageMapEntry>,
    pub provisioned_vault_partitions: Vec<PathBuf>,
}

pub fn discover_workspace(
    workspace_root: &Path,
    code_graph_store: &GraphStore,
) -> Result<WorkspaceDiscoveryReport, DaemonError> {
    discover_workspace_with_vault(workspace_root, None, code_graph_store)
}

pub fn discover_workspace_with_vault(
    workspace_root: &Path,
    vault_root: Option<&Path>,
    code_graph_store: &GraphStore,
) -> Result<WorkspaceDiscoveryReport, DaemonError> {
    let mut repo_paths = Vec::new();

    if workspace_root.join(".git").exists() {
        repo_paths.push(workspace_root.to_path_buf());
    }

    let walker = WalkDir::new(workspace_root)
        .max_depth(4)
        .into_iter()
        .filter_entry(|entry| {
            if entry.depth() == 0 {
                return true;
            }
            let name = entry.file_name().to_string_lossy();
            if name.starts_with('.') {
                return false;
            }
            if name == "target" || name == "node_modules" || name == "out" || name == "build" {
                return false;
            }
            true
        });

    for entry in walker.filter_map(|e| e.ok()) {
        if entry.file_type().is_dir() {
            let path = entry.path();
            if path != workspace_root && path.join(".git").exists() {
                repo_paths.push(path.to_path_buf());
            }
        }
    }

    repo_paths.sort();
    repo_paths.dedup();

    let mut report = WorkspaceDiscoveryReport::default();

    for repo_path in repo_paths {
        let (repo_rec, pkgs) = discover_and_index_repository(code_graph_store, &repo_path)
            .map_err(|e| DaemonError::Engine(e.to_string()))?;

        if let Some(vroot) = vault_root {
            let repo_partition = vroot.join("repositories").join(&repo_rec.repo_name);
            for sub in &["atoms", "molecules", "organisms", "incidents", "decisions"] {
                let sub_dir = repo_partition.join(sub);
                let _ = std::fs::create_dir_all(&sub_dir);
            }
            report.provisioned_vault_partitions.push(repo_partition);
        }

        report.discovered_repositories.push(repo_rec);
        report.package_mappings.extend(pkgs);
    }

    Ok(report)
}

#[derive(Debug, Clone)]
pub struct ReconcileOutcome {
    pub project: String,
    pub events_processed: usize,
    pub code_graph_reconciled: bool,
    pub vault_reindexed: bool,
    pub watermark_updated: Option<WatermarkRecord>,
}

fn resolve_repo_path(
    project: &str,
    workspace_root: &Path,
    code_graph_store: &GraphStore,
) -> Option<PathBuf> {
    // 1. Direct subpath match
    let direct = workspace_root.join(project);
    if direct.exists() && direct.is_dir() {
        return Some(direct);
    }

    // 2. Leaf repo name
    let leaf_name = project.rsplit('/').next().unwrap_or(project);
    let leaf_path = workspace_root.join(leaf_name);
    if leaf_path.exists() && leaf_path.is_dir() {
        return Some(leaf_path);
    }

    // 3. Match from GraphStore registered repositories
    if let Ok(Some(repo)) = code_graph_store.get_repository(project) {
        let p = PathBuf::from(&repo.worktree_path);
        if p.exists() {
            return Some(p);
        }
    }
    if let Ok(Some(repo)) = code_graph_store.get_repository(leaf_name) {
        let p = PathBuf::from(&repo.worktree_path);
        if p.exists() {
            return Some(p);
        }
    }

    None
}

pub fn reconcile_project_batch(
    batch: &CoalescedProjectBatch,
    workspace_root: &Path,
    code_graph_store: &GraphStore,
    vault_root: Option<&Path>,
    watermark_store: Option<&WatermarkStore>,
    stream_source: &str,
) -> Result<ReconcileOutcome, DaemonError> {
    let mut outcome = ReconcileOutcome {
        project: batch.project.clone(),
        events_processed: batch.events.len(),
        code_graph_reconciled: false,
        vault_reindexed: false,
        watermark_updated: None,
    };

    let maybe_repo_path = resolve_repo_path(&batch.project, workspace_root, code_graph_store);

    if let Some(repo_path) = maybe_repo_path {
        let repo_name = repo_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| batch.project.clone());

        let _ = discover_and_index_repository(code_graph_store, &repo_path);

        if let Some(ref rev) = batch.latest_revision {
            let sanitizer = CodeProfile::default();
            let embedding = OfflineEmbeddingProvider::default();
            let _ = index_repository_at_rev(
                code_graph_store,
                &repo_path,
                &repo_name,
                rev,
                &sanitizer,
                &embedding,
                0.75,
            );
        }

        outcome.code_graph_reconciled = true;
    }

    if let Some(vroot) = vault_root {
        if vroot.exists() {
            if let Ok(graph) = VaultGraph::from_dir(vroot) {
                if let Ok(mut vindex) = VaultIndex::open_in_vault(vroot) {
                    let sanitizer = CodeProfile::default();
                    let embedding = OfflineEmbeddingProvider::default();
                    let _ = vindex.reindex(&graph, Some(&sanitizer), Some(&embedding));
                    outcome.vault_reindexed = true;
                }
            }
        }
    }

    if let Some(wstore) = watermark_store {
        let ts = batch.latest_timestamp.unwrap_or(0);
        let cid = batch.latest_change_id.as_deref().unwrap_or("unknown");
        let record = wstore.advance_watermark(stream_source, ts, cid)?;
        outcome.watermark_updated = Some(record);
    }

    Ok(outcome)
}
