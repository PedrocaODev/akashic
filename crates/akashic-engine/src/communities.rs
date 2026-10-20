use crate::store::{GraphStore, CommunityRecord, ConfidenceTier};
use anyhow::Result;
use std::collections::HashMap;

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
    
    let mut file_buckets: HashMap<&str, Vec<usize>> = HashMap::new();
    for (idx, sym) in symbols.iter().enumerate() {
        file_buckets.entry(sym.file_path.as_str()).or_default().push(idx);
    }

    for indices in file_buckets.values() {
        for (pos, &i) in indices.iter().enumerate() {
            for &j in &indices[(pos + 1)..] {
                let s1 = &symbols[i];
                let s2 = &symbols[j];
                *adj.entry(s1.id.clone()).or_default().entry(s2.id.clone()).or_default() += 0.3;
                *adj.entry(s2.id.clone()).or_default().entry(s1.id.clone()).or_default() += 0.3;
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
            
            if let Some((&best_comm, _)) = comm_weights.iter().max_by(|a, b| a.1.total_cmp(b.1).then_with(|| b.0.cmp(a.0))) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::CodeSymbol;

    #[test]
    fn test_file_bucketed_community_detection() {
        let store = GraphStore::open_in_memory().unwrap();
        let repo = "repo_bucket_test";

        let sym1 = CodeSymbol {
            id: "repo::f1.rs::s1".into(),
            repo_name: repo.into(),
            file_path: "f1.rs".into(),
            symbol_name: "s1".into(),
            kind: "function".into(),
            signature: "fn s1()".into(),
            visibility: "pub".into(),
            start_line: 1,
            end_line: 5,
            doc_comment: "".into(),
            community_id: None,
            commit_sha: "head".into(),
        };
        let sym2 = CodeSymbol {
            id: "repo::f1.rs::s2".into(),
            repo_name: repo.into(),
            file_path: "f1.rs".into(),
            symbol_name: "s2".into(),
            kind: "function".into(),
            signature: "fn s2()".into(),
            visibility: "pub".into(),
            start_line: 7,
            end_line: 10,
            doc_comment: "".into(),
            community_id: None,
            commit_sha: "head".into(),
        };
        let sym3 = CodeSymbol {
            id: "repo::f2.rs::s3".into(),
            repo_name: repo.into(),
            file_path: "f2.rs".into(),
            symbol_name: "s3".into(),
            kind: "function".into(),
            signature: "fn s3()".into(),
            visibility: "pub".into(),
            start_line: 1,
            end_line: 5,
            doc_comment: "".into(),
            community_id: None,
            commit_sha: "head".into(),
        };
        let sym4 = CodeSymbol {
            id: "repo::f2.rs::s4".into(),
            repo_name: repo.into(),
            file_path: "f2.rs".into(),
            symbol_name: "s4".into(),
            kind: "function".into(),
            signature: "fn s4()".into(),
            visibility: "pub".into(),
            start_line: 7,
            end_line: 10,
            doc_comment: "".into(),
            community_id: None,
            commit_sha: "head".into(),
        };

        store.upsert_symbol(&sym1).unwrap();
        store.upsert_symbol(&sym2).unwrap();
        store.upsert_symbol(&sym3).unwrap();
        store.upsert_symbol(&sym4).unwrap();

        let comms = detect_communities(&store, repo).unwrap();
        assert!(!comms.is_empty());

        let sym1_updated = store.get_symbol_by_id(&sym1.id).unwrap().unwrap();
        let sym2_updated = store.get_symbol_by_id(&sym2.id).unwrap().unwrap();
        let sym3_updated = store.get_symbol_by_id(&sym3.id).unwrap().unwrap();
        let sym4_updated = store.get_symbol_by_id(&sym4.id).unwrap().unwrap();

        assert_eq!(sym1_updated.community_id, sym2_updated.community_id);
        assert_eq!(sym3_updated.community_id, sym4_updated.community_id);
        assert_ne!(sym1_updated.community_id, sym3_updated.community_id);
    }
}
