use crate::store::{GraphStore, CommunityRecord, ConfidenceTier};
use anyhow::Result;
use std::collections::{HashMap, HashSet};

pub fn detect_communities(store: &GraphStore, repo_name: &str) -> Result<Vec<CommunityRecord>> {
    let symbols = store.get_symbols_by_repo(repo_name)?;
    let edges = store.get_edges_by_repo(repo_name)?;
    
    let mut adj: HashMap<String, HashMap<String, f64>> = HashMap::new();
    
    for edge in &edges {
        let weight = match edge.confidence_tier {
            ConfidenceTier::Extracted => 1.0,
            ConfidenceTier::Inferred => edge.confidence_score * 0.5,
            ConfidenceTier::Ambiguous => 0.25,
        };
        *adj.entry(edge.source_id.clone()).or_default().entry(edge.target_id.clone()).or_default() += weight;
        *adj.entry(edge.target_id.clone()).or_default().entry(edge.source_id.clone()).or_default() += weight;
    }
    
    for i in 0..symbols.len() {
        for j in (i+1)..symbols.len() {
            let s1 = &symbols[i];
            let s2 = &symbols[j];
            if s1.file_path == s2.file_path {
                *adj.entry(s1.id.clone()).or_default().entry(s2.id.clone()).or_default() += 0.3;
                *adj.entry(s2.id.clone()).or_default().entry(s1.id.clone()).or_default() += 0.3;
            } else {
                let pkg1: Vec<&str> = s1.symbol_name.split('.').take(3).collect();
                let pkg2: Vec<&str> = s2.symbol_name.split('.').take(3).collect();
                if !pkg1.is_empty() && pkg1 == pkg2 {
                    *adj.entry(s1.id.clone()).or_default().entry(s2.id.clone()).or_default() += 0.1;
                    *adj.entry(s2.id.clone()).or_default().entry(s1.id.clone()).or_default() += 0.1;
                }
            }
        }
    }
    
    let mut node_community: HashMap<String, usize> = HashMap::new();
    let mut sorted_ids: Vec<String> = symbols.iter().map(|s| s.id.clone()).collect();
    sorted_ids.sort();
    
    for (idx, id) in sorted_ids.iter().enumerate() {
        node_community.insert(id.clone(), idx);
    }
    
    let mut changed = true;
    let max_iter = 10;
    let mut iter = 0;
    
    while changed && iter < max_iter {
        changed = false;
        iter += 1;
        
        for id in &sorted_ids {
            let current_comm = node_community[id];
            let mut comm_weights: HashMap<usize, f64> = HashMap::new();
            
            if let Some(neighbors) = adj.get(id) {
                for (neighbor, weight) in neighbors {
                    if let Some(&nc) = node_community.get(neighbor) {
                        *comm_weights.entry(nc).or_default() += weight;
                    }
                }
            }
            
            if let Some((&best_comm, _)) = comm_weights.iter().max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal)) {
                if best_comm != current_comm {
                    node_community.insert(id.clone(), best_comm);
                    changed = true;
                }
            }
        }
    }
    
    let mut comm_nodes_map: HashMap<usize, Vec<String>> = HashMap::new();
    for (id, comm) in &node_community {
        comm_nodes_map.entry(*comm).or_default().push(id.clone());
    }
    
    // Sort nodes in each community, and then sort communities by their smallest node ID for deterministic assignment
    let mut comm_nodes: Vec<Vec<String>> = comm_nodes_map.into_values().filter(|v| !v.is_empty()).collect();
    for nodes in &mut comm_nodes {
        nodes.sort();
    }
    comm_nodes.sort_by(|a, b| a[0].cmp(&b[0]));
    
    store.clear_communities_by_repo(repo_name)?;
    
    let mut results = Vec::new();
    let id_to_symbol: HashMap<String, _> = symbols.into_iter().map(|s| (s.id.clone(), s)).collect();
    
    for (comm_idx, nodes) in comm_nodes.into_iter().enumerate() {
        let community_id = format!("{}::comm-{:03}", repo_name, comm_idx);
        
        let mut internal_weight = 0.0;
        let mut total_weight = 0.0;
        let mut file_counts: HashMap<String, usize> = HashMap::new();
        
        for node in &nodes {
            if let Some(sym) = id_to_symbol.get(node) {
                *file_counts.entry(sym.file_path.clone()).or_default() += 1;
            }
            if let Some(neighbors) = adj.get(node) {
                for (neighbor, weight) in neighbors {
                    total_weight += weight;
                    if nodes.contains(neighbor) {
                        internal_weight += weight;
                    }
                }
            }
        }
        
        let cohesion_score = if total_weight > 0.0 { internal_weight / total_weight } else { 1.0 };
        
        let label = file_counts.into_iter()
            .max_by_key(|&(_, count)| count)
            .map(|(path, _)| path)
            .unwrap_or_else(|| "Unknown".to_string());
            
        let record = CommunityRecord {
            community_id: community_id.clone(),
            repo_name: repo_name.to_string(),
            label,
            cohesion_score,
            node_count: nodes.len(),
        };
        
        store.upsert_community(&record)?;
        
        for node in nodes {
            store.update_symbol_community(&node, Some(community_id.as_str()))?;
        }
        
        results.push(record);
    }
    
    Ok(results)
}
