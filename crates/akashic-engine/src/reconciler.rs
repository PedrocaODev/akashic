use anyhow::Result;
use std::path::Path;
use akashic_sanitize::SanitizerProfile;
use akashic_embedding::EmbeddingProvider;
use crate::store::{GraphStore, CodeEdge, ConfidenceTier};
use crate::ingestor::{diff_commits, read_blob_at_commit, walk_worktree_files, resolve_commit_sha, list_tree_files};
use crate::ast::{extract_file_graph, resolve_intra_repo_edges};
use crate::filter::{evaluate_file_significance, SignificanceVerdict};
use crate::search::compute_embeddings_and_inferred_edges;

pub struct ReconcileReport {
    pub repo_name: String,
    pub commit_sha: String,
    pub files_inspected: usize,
    pub files_indexed: usize,
    pub files_skipped_impl_only: usize,
    pub symbols_upserted: usize,
    pub symbols_removed: usize,
    pub extracted_edges: usize,
    pub inferred_edges: usize,
}

fn finalize_indexing(
    store: &GraphStore,
    repo_name: &str,
    commit_sha: &str,
    report: &mut ReconcileReport,
    embedding: &impl EmbeddingProvider,
    threshold: f32,
) -> Result<()> {
    let all_symbols = store.get_symbols_by_repo(repo_name)?;
    let mut all_edges = store.get_edges_by_repo(repo_name)?;
    
    // Resolve intra repo edges
    resolve_intra_repo_edges(&all_symbols, &mut all_edges);
    
    // Upsert resolved edges
    for edge in all_edges {
        if edge.confidence_tier == ConfidenceTier::Extracted {
            store.upsert_edge(&edge)?;
        }
    }
    
    // Update repository record with last_indexed_sha
    if let Ok(Some(mut repo)) = store.get_repository(repo_name) {
        repo.last_indexed_sha = commit_sha.to_string();
        store.upsert_repository(&repo)?;
    }
    
    report.inferred_edges = compute_embeddings_and_inferred_edges(store, repo_name, embedding, threshold)?;
    Ok(())
}

pub fn index_repository_at_rev<S: SanitizerProfile, E: EmbeddingProvider>(
    store: &GraphStore,
    repo_path: &Path,
    repo_name: &str,
    rev: &str,
    sanitizer: &S,
    embedding: &E,
    threshold: f32,
) -> Result<ReconcileReport> {
    let mut report = ReconcileReport {
        repo_name: repo_name.to_string(),
        commit_sha: rev.to_string(),
        files_inspected: 0,
        files_indexed: 0,
        files_skipped_impl_only: 0,
        symbols_upserted: 0,
        symbols_removed: 0,
        extracted_edges: 0,
        inferred_edges: 0,
    };

    let files = match list_tree_files(repo_path, rev) {
        Ok(f) => f,
        Err(_) => walk_worktree_files(repo_path)?,
    };

    let actual_rev = resolve_commit_sha(repo_path, rev).unwrap_or_else(|_| rev.to_string());
    report.commit_sha = actual_rev.clone();

    for file_path in files {
        report.files_inspected += 1;
        let source = if file_path.contains(".git") { continue; } else {
            read_blob_at_commit(repo_path, &actual_rev, &file_path)
                .unwrap_or_else(|_| std::fs::read_to_string(repo_path.join(&file_path)).unwrap_or_default())
        };

        if source.is_empty() { continue; }

        let graph = extract_file_graph(repo_name, &file_path, &source, &actual_rev, sanitizer)?;
        let verdict = evaluate_file_significance(&file_path, None, Some(&graph));

        match verdict {
            SignificanceVerdict::ExcludedPath | SignificanceVerdict::ImplementationOnly | SignificanceVerdict::NoSemanticChange => {
                if let SignificanceVerdict::ImplementationOnly = verdict {
                    report.files_skipped_impl_only += 1;
                }
            },
            SignificanceVerdict::StructurallySignificant { .. } => {
                store.delete_symbols_by_file(repo_name, &file_path)?;
                store.delete_edges_by_file(repo_name, &file_path)?;
                
                for sym in &graph.symbols {
                    store.upsert_symbol(sym)?;
                    report.symbols_upserted += 1;
                }
                for edge in &graph.edges {
                    store.upsert_edge(edge)?;
                    report.extracted_edges += 1;
                }
                report.files_indexed += 1;
            }
        }
    }

    finalize_indexing(store, repo_name, &actual_rev, &mut report, embedding, threshold)?;
    Ok(report)
}

pub fn reconcile_incremental<S: SanitizerProfile, E: EmbeddingProvider>(
    store: &GraphStore,
    repo_path: &Path,
    repo_name: &str,
    old_rev: &str,
    new_rev: &str,
    sanitizer: &S,
    embedding: &E,
    threshold: f32,
) -> Result<ReconcileReport> {
    let mut report = ReconcileReport {
        repo_name: repo_name.to_string(),
        commit_sha: new_rev.to_string(),
        files_inspected: 0,
        files_indexed: 0,
        files_skipped_impl_only: 0,
        symbols_upserted: 0,
        symbols_removed: 0,
        extracted_edges: 0,
        inferred_edges: 0,
    };

    let actual_new_rev = resolve_commit_sha(repo_path, new_rev).unwrap_or_else(|_| new_rev.to_string());
    let actual_old_rev = resolve_commit_sha(repo_path, old_rev).unwrap_or_else(|_| old_rev.to_string());
    report.commit_sha = actual_new_rev.clone();

    let diffs = diff_commits(repo_path, &actual_old_rev, &actual_new_rev)?;

    for diff in diffs {
        report.files_inspected += 1;
        let file_path = diff.path;
        
        let mut before = None;
        let mut after = None;
        
        if diff.status != "A" {
            let old_source = read_blob_at_commit(repo_path, &actual_old_rev, &file_path).unwrap_or_default();
            if !old_source.is_empty() {
                before = Some(extract_file_graph(repo_name, &file_path, &old_source, &actual_old_rev, sanitizer)?);
            }
        }
        
        if diff.status != "D" {
            let new_source = read_blob_at_commit(repo_path, &actual_new_rev, &file_path)
                .unwrap_or_else(|_| std::fs::read_to_string(repo_path.join(&file_path)).unwrap_or_default());
            if !new_source.is_empty() {
                after = Some(extract_file_graph(repo_name, &file_path, &new_source, &actual_new_rev, sanitizer)?);
            }
        }
        
        let verdict = evaluate_file_significance(&file_path, before.as_ref(), after.as_ref());
        
        match verdict {
            SignificanceVerdict::ExcludedPath | SignificanceVerdict::ImplementationOnly | SignificanceVerdict::NoSemanticChange => {
                if let SignificanceVerdict::ImplementationOnly = verdict {
                    report.files_skipped_impl_only += 1;
                }
            },
            SignificanceVerdict::StructurallySignificant { .. } => {
                store.delete_symbols_by_file(repo_name, &file_path)?;
                store.delete_edges_by_file(repo_name, &file_path)?;
                
                if let Some(graph) = after {
                    for sym in &graph.symbols {
                        store.upsert_symbol(sym)?;
                        report.symbols_upserted += 1;
                    }
                    for edge in &graph.edges {
                        store.upsert_edge(edge)?;
                        report.extracted_edges += 1;
                    }
                } else if before.is_some() {
                    report.symbols_removed += before.unwrap().symbols.len();
                }
                report.files_indexed += 1;
            }
        }
    }

    finalize_indexing(store, repo_name, &actual_new_rev, &mut report, embedding, threshold)?;
    Ok(report)
}
