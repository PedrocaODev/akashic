use crate::store::{GraphStore, CodeSymbol, CodeEdge, CommunityRecord, ConfidenceTier};
use anyhow::Result;
use std::collections::{HashMap, HashSet, VecDeque};

pub struct GodNodeEntry {
    pub symbol: CodeSymbol,
    pub in_degree: usize,
    pub out_degree: usize,
    pub total_degree: usize,
}

pub fn god_nodes(store: &GraphStore, repo_name: &str, top_n: usize) -> Result<Vec<GodNodeEntry>> {
    let symbols = store.get_symbols_by_repo(repo_name)?;
    let edges = store.get_edges_by_repo(repo_name)?;
    
    let mut in_degrees: HashMap<String, usize> = HashMap::new();
    let mut out_degrees: HashMap<String, usize> = HashMap::new();
    
    for edge in &edges {
        *out_degrees.entry(edge.source_id.clone()).or_default() += 1;
        *in_degrees.entry(edge.target_id.clone()).or_default() += 1;
    }
    
    let mut entries: Vec<GodNodeEntry> = symbols.into_iter().map(|symbol| {
        let in_deg = *in_degrees.get(&symbol.id).unwrap_or(&0);
        let out_deg = *out_degrees.get(&symbol.id).unwrap_or(&0);
        GodNodeEntry {
            symbol,
            in_degree: in_deg,
            out_degree: out_deg,
            total_degree: in_deg + out_deg,
        }
    }).collect();
    
    entries.sort_by(|a, b| b.total_degree.cmp(&a.total_degree).then_with(|| a.symbol.id.cmp(&b.symbol.id)));
    entries.truncate(top_n);
    
    Ok(entries)
}

pub struct SurprisingConnection {
    pub edge: CodeEdge,
    pub source_symbol: String,
    pub target_symbol: String,
    pub source_community: Option<String>,
    pub target_community: Option<String>,
    pub surprise_score: f64,
    pub reason: String,
}

pub fn surprising_connections(store: &GraphStore, repo_name: &str, top_n: usize) -> Result<Vec<SurprisingConnection>> {
    let symbols = store.get_symbols_by_repo(repo_name)?;
    let edges = store.get_edges_by_repo(repo_name)?;
    
    let sym_map: HashMap<String, CodeSymbol> = symbols.into_iter().map(|s| (s.id.clone(), s)).collect();
    
    let mut connections = Vec::new();
    
    for edge in edges {
        let src = match sym_map.get(&edge.source_id) { Some(s) => s, None => continue };
        let tgt = match sym_map.get(&edge.target_id) { Some(t) => t, None => continue };
        
        let mut score = 0.0;
        let mut reasons = Vec::new();
        
        if src.community_id != tgt.community_id {
            score += 0.5;
            reasons.push("Cross-community bridge".to_string());
        }
        
        if edge.confidence_tier == ConfidenceTier::Inferred {
            score += edge.confidence_score;
            reasons.push("Inferred semantic connection".to_string());
        }
        
        if score > 0.0 {
            connections.push(SurprisingConnection {
                edge,
                source_symbol: src.symbol_name.clone(),
                target_symbol: tgt.symbol_name.clone(),
                source_community: src.community_id.clone(),
                target_community: tgt.community_id.clone(),
                surprise_score: score,
                reason: reasons.join(", "),
            });
        }
    }
    
    connections.sort_by(|a, b| b.surprise_score.partial_cmp(&a.surprise_score).unwrap_or(std::cmp::Ordering::Equal));
    connections.truncate(top_n);
    
    Ok(connections)
}

pub struct PathStep {
    pub from_id: String,
    pub to_id: String,
    pub edge_type: String,
    pub confidence_tier: ConfidenceTier,
    pub confidence_score: f64,
}

pub fn shortest_path(store: &GraphStore, repo_name: &str, source_id: &str, target_id: &str, max_hops: usize) -> Result<Option<Vec<PathStep>>> {
    let edges = store.get_edges_by_repo(repo_name)?;
    let mut adj: HashMap<String, Vec<CodeEdge>> = HashMap::new();
    for edge in edges {
        adj.entry(edge.source_id.clone()).or_default().push(edge);
    }
    
    let mut queue = VecDeque::new();
    let mut visited = HashSet::new();
    let mut parent: HashMap<String, (String, CodeEdge)> = HashMap::new();
    
    queue.push_back((source_id.to_string(), 0));
    visited.insert(source_id.to_string());
    
    while let Some((curr, depth)) = queue.pop_front() {
        if curr == target_id {
            let mut path = Vec::new();
            let mut node = curr;
            while node != source_id {
                let (prev, edge) = parent.remove(&node).unwrap();
                path.push(PathStep {
                    from_id: prev.clone(),
                    to_id: node,
                    edge_type: edge.edge_type,
                    confidence_tier: edge.confidence_tier,
                    confidence_score: edge.confidence_score,
                });
                node = prev;
            }
            path.reverse();
            return Ok(Some(path));
        }
        
        if depth >= max_hops { continue; }
        
        if let Some(neighbors) = adj.get(&curr) {
            for edge in neighbors {
                if !visited.contains(&edge.target_id) {
                    visited.insert(edge.target_id.clone());
                    parent.insert(edge.target_id.clone(), (curr.clone(), edge.clone()));
                    queue.push_back((edge.target_id.clone(), depth + 1));
                }
            }
        }
    }
    
    Ok(None)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraversalMode {
    Bfs,
    Dfs,
}

pub struct TraversalResult {
    pub root_id: String,
    pub mode: TraversalMode,
    pub visited_symbols: Vec<CodeSymbol>,
    pub traversed_edges: Vec<CodeEdge>,
    pub estimated_tokens: usize,
    pub truncated_by_budget: bool,
}

pub fn traverse_neighborhood(store: &GraphStore, root_id: &str, mode: TraversalMode, max_hops: usize, token_budget: usize) -> Result<TraversalResult> {
    let root = store.get_symbol_by_id(root_id)?.ok_or_else(|| anyhow::anyhow!("Root symbol not found"))?;
    
    let mut visited = HashSet::new();
    let mut symbols = Vec::new();
    let mut traversed_edges = Vec::new();
    
    let mut queue_bfs = VecDeque::new();
    let mut stack_dfs = Vec::new();
    
    if mode == TraversalMode::Bfs {
        queue_bfs.push_back((root_id.to_string(), 0));
    } else {
        stack_dfs.push((root_id.to_string(), 0));
    }
    
    visited.insert(root_id.to_string());
    symbols.push(root);
    
    let mut current_tokens = 0;
    let mut truncated = false;
    
    loop {
        let (curr, depth) = match mode {
            TraversalMode::Bfs => match queue_bfs.pop_front() { Some(v) => v, None => break },
            TraversalMode::Dfs => match stack_dfs.pop() { Some(v) => v, None => break },
        };
        
        if depth >= max_hops { continue; }
        
        let out_edges = store.get_outgoing_edges(&curr)?;
        for edge in out_edges {
            if !visited.contains(&edge.target_id) {
                if let Some(sym) = store.get_symbol_by_id(&edge.target_id)? {
                    let cost = (sym.signature.len() + sym.doc_comment.len() + sym.symbol_name.len()) / 4 + 10;
                    if current_tokens + cost > token_budget {
                        truncated = true;
                        continue;
                    }
                    current_tokens += cost;
                    visited.insert(edge.target_id.clone());
                    symbols.push(sym);
                    traversed_edges.push(edge.clone());
                    
                    if mode == TraversalMode::Bfs {
                        queue_bfs.push_back((edge.target_id.clone(), depth + 1));
                    } else {
                        stack_dfs.push((edge.target_id.clone(), depth + 1));
                    }
                }
            }
        }
    }
    
    Ok(TraversalResult {
        root_id: root_id.to_string(),
        mode,
        visited_symbols: symbols,
        traversed_edges,
        estimated_tokens: current_tokens,
        truncated_by_budget: truncated,
    })
}

pub struct BlastRadiusReport {
    pub changed_symbols: Vec<String>,
    pub upstream_callers: Vec<CodeSymbol>,
    pub downstream_dependencies: Vec<CodeSymbol>,
    pub impacted_communities: Vec<CommunityRecord>,
    pub max_depth_reached: usize,
}

pub fn blast_radius(store: &GraphStore, repo_name: &str, symbol_ids: &[String], max_depth: usize) -> Result<BlastRadiusReport> {
    let mut up_queue = VecDeque::new();
    let mut down_queue = VecDeque::new();
    
    let mut visited_up = HashSet::new();
    let mut visited_down = HashSet::new();
    
    for id in symbol_ids {
        up_queue.push_back((id.clone(), 0));
        down_queue.push_back((id.clone(), 0));
        visited_up.insert(id.clone());
        visited_down.insert(id.clone());
    }
    
    let mut upstream_callers = Vec::new();
    let mut downstream_dependencies = Vec::new();
    let mut actual_depth = 0;
    
    while let Some((curr, depth)) = up_queue.pop_front() {
        actual_depth = actual_depth.max(depth);
        if depth >= max_depth { continue; }
        
        for edge in store.get_incoming_edges(&curr)? {
            if !visited_up.contains(&edge.source_id) {
                visited_up.insert(edge.source_id.clone());
                if let Some(sym) = store.get_symbol_by_id(&edge.source_id)? {
                    upstream_callers.push(sym);
                }
                up_queue.push_back((edge.source_id.clone(), depth + 1));
            }
        }
    }
    
    while let Some((curr, depth)) = down_queue.pop_front() {
        actual_depth = actual_depth.max(depth);
        if depth >= max_depth { continue; }
        
        for edge in store.get_outgoing_edges(&curr)? {
            if !visited_down.contains(&edge.target_id) {
                visited_down.insert(edge.target_id.clone());
                if let Some(sym) = store.get_symbol_by_id(&edge.target_id)? {
                    downstream_dependencies.push(sym);
                }
                down_queue.push_back((edge.target_id.clone(), depth + 1));
            }
        }
    }
    
    let mut impacted_comm_ids = HashSet::new();
    for sym in upstream_callers.iter().chain(downstream_dependencies.iter()) {
        if let Some(cid) = &sym.community_id {
            impacted_comm_ids.insert(cid.clone());
        }
    }
    
    let all_comms = store.get_communities(repo_name)?;
    let impacted_communities: Vec<_> = all_comms.into_iter()
        .filter(|c| impacted_comm_ids.contains(&c.community_id))
        .collect();
        
    Ok(BlastRadiusReport {
        changed_symbols: symbol_ids.to_vec(),
        upstream_callers,
        downstream_dependencies,
        impacted_communities,
        max_depth_reached: actual_depth,
    })
}
