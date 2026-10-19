use crate::store::{GraphStore, RepositoryRecord, PackageMapEntry, CodeSymbol, CodeEdge, CommunityRecord};
use anyhow::Result;
use serde::{Serialize, Deserialize};
use std::path::Path;
use std::fs;

#[derive(Serialize, Deserialize, Clone)]
pub struct GraphProjection {
    pub schema_version: u32,
    pub repo_name: String,
    pub commit_sha: String,
    pub repository: Option<RepositoryRecord>,
    pub package_map: Vec<PackageMapEntry>,
    pub symbols: Vec<CodeSymbol>,
    pub edges: Vec<CodeEdge>,
    pub communities: Vec<CommunityRecord>,
}

pub fn export_projection(store: &GraphStore, repo_name: &str, output_path: &Path) -> Result<GraphProjection> {
    let repo = store.get_repository(repo_name)?;
    let package_map = store.get_package_map(repo_name)?;
    let symbols = store.get_symbols_by_repo(repo_name)?;
    let edges = store.get_edges_by_repo(repo_name)?;
    let communities = store.get_communities(repo_name)?;
    
    let commit_sha = repo.as_ref().map(|r| r.last_indexed_sha.clone()).unwrap_or_else(|| "unknown".to_string());
    
    let proj = GraphProjection {
        schema_version: 1,
        repo_name: repo_name.to_string(),
        commit_sha,
        repository: repo,
        package_map,
        symbols,
        edges,
        communities,
    };
    
    let json = serde_json::to_string_pretty(&proj)?;
    let temp_path = output_path.with_extension("tmp");
    fs::write(&temp_path, json)?;
    fs::rename(temp_path, output_path)?;
    
    Ok(proj)
}

pub fn import_projection(store: &GraphStore, input_path: &Path) -> Result<GraphProjection> {
    let json = fs::read_to_string(input_path)?;
    let proj: GraphProjection = serde_json::from_str(&json)?;
    
    if let Some(repo) = &proj.repository {
        store.upsert_repository(repo)?;
    }
    
    store.clear_package_map_by_repo(&proj.repo_name)?;
    for pkg in &proj.package_map {
        store.upsert_package_map(pkg)?;
    }
    
    store.clear_communities_by_repo(&proj.repo_name)?;
    for comm in &proj.communities {
        store.upsert_community(comm)?;
    }
    
    store.delete_symbols_by_repo(&proj.repo_name)?;
    for sym in &proj.symbols {
        store.upsert_symbol(sym)?;
    }
    
    store.delete_edges_by_repo(&proj.repo_name)?;
    for edge in &proj.edges {
        store.upsert_edge(edge)?;
    }
    
    Ok(proj)
}
