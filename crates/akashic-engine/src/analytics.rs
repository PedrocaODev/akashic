use crate::store::{GraphStore, CodeSymbol, CodeEdge, CommunityRecord, ConfidenceTier};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};

impl PartialEq for CodeEdge {
    fn eq(&self, other: &Self) -> bool {
        self.source_id == other.source_id
            && self.target_id == other.target_id
            && self.edge_type == other.edge_type
            && self.confidence_tier == other.confidence_tier
            && self.confidence_score.to_bits() == other.confidence_score.to_bits()
            && self.evidence == other.evidence
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GodNodeEntry {
    pub symbol: CodeSymbol,
    pub in_degree: usize,
    pub out_degree: usize,
    pub total_degree: usize,
    pub betweenness_centrality: f64,
}

pub type GodNode = GodNodeEntry;

pub fn compute_brandes_betweenness(n: usize, adj: &[Vec<usize>]) -> Vec<f64> {
    let mut cb = vec![0.0; n];

    for s in 0..n {
        let mut stack = Vec::with_capacity(n);
        let mut pred: Vec<Vec<usize>> = vec![Vec::new(); n];
        let mut sigma = vec![0.0; n];
        let mut dist = vec![-1i64; n];

        sigma[s] = 1.0;
        dist[s] = 0;

        let mut queue = VecDeque::new();
        queue.push_back(s);

        while let Some(v) = queue.pop_front() {
            stack.push(v);
            if let Some(neighbors) = adj.get(v) {
                for &w in neighbors {
                    if dist[w] < 0 {
                        dist[w] = dist[v] + 1;
                        queue.push_back(w);
                    }
                    if dist[w] == dist[v] + 1 {
                        sigma[w] += sigma[v];
                        pred[w].push(v);
                    }
                }
            }
        }

        let mut delta = vec![0.0; n];
        while let Some(w) = stack.pop() {
            for &v in &pred[w] {
                delta[v] += (sigma[v] / sigma[w]) * (1.0 + delta[w]);
            }
            if w != s {
                cb[w] += delta[w];
            }
        }
    }

    for val in &mut cb {
        *val /= 2.0;
    }

    cb
}

pub fn god_nodes(store: &GraphStore, repo_name: &str, top_n: usize) -> Result<Vec<GodNodeEntry>> {
    if top_n == 0 {
        return Ok(Vec::new());
    }

    let symbols = store.get_symbols_by_repo(repo_name)?;
    let edges = store.get_edges_by_repo(repo_name)?;

    let n = symbols.len();
    let id_to_idx: HashMap<&str, usize> = symbols
        .iter()
        .enumerate()
        .map(|(i, s)| (s.id.as_str(), i))
        .collect();

    let mut in_degrees: HashMap<String, usize> = HashMap::new();
    let mut out_degrees: HashMap<String, usize> = HashMap::new();
    let mut undirected_adj: Vec<HashSet<usize>> = vec![HashSet::new(); n];

    for edge in &edges {
        *out_degrees.entry(edge.source_id.clone()).or_default() += 1;
        *in_degrees.entry(edge.target_id.clone()).or_default() += 1;

        if let (Some(&u), Some(&v)) = (
            id_to_idx.get(edge.source_id.as_str()),
            id_to_idx.get(edge.target_id.as_str()),
        ) {
            if u != v {
                undirected_adj[u].insert(v);
                undirected_adj[v].insert(u);
            }
        }
    }

    let adj: Vec<Vec<usize>> = undirected_adj
        .into_iter()
        .map(|s| {
            let mut neighbors: Vec<usize> = s.into_iter().collect();
            neighbors.sort_unstable();
            neighbors
        })
        .collect();

    let betweenness = compute_brandes_betweenness(n, &adj);

    let mut entries: Vec<GodNodeEntry> = symbols
        .into_iter()
        .enumerate()
        .map(|(idx, symbol)| {
            let in_deg = *in_degrees.get(&symbol.id).unwrap_or(&0);
            let out_deg = *out_degrees.get(&symbol.id).unwrap_or(&0);
            let bc = betweenness.get(idx).copied().unwrap_or(0.0);
            GodNodeEntry {
                symbol,
                in_degree: in_deg,
                out_degree: out_deg,
                total_degree: in_deg + out_deg,
                betweenness_centrality: bc,
            }
        })
        .collect();

    let cmp = |a: &GodNodeEntry, b: &GodNodeEntry| {
        b.total_degree
            .cmp(&a.total_degree)
            .then_with(|| b.betweenness_centrality.total_cmp(&a.betweenness_centrality))
            .then_with(|| a.symbol.id.cmp(&b.symbol.id))
    };

    if entries.len() > top_n {
        entries.select_nth_unstable_by(top_n - 1, cmp);
        entries.truncate(top_n);
    }
    entries.sort_unstable_by(cmp);

    Ok(entries)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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
    if top_n == 0 {
        return Ok(Vec::new());
    }

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

    let cmp = |a: &SurprisingConnection, b: &SurprisingConnection| {
        b.surprise_score
            .total_cmp(&a.surprise_score)
            .then_with(|| a.edge.source_id.cmp(&b.edge.source_id))
            .then_with(|| a.edge.target_id.cmp(&b.edge.target_id))
            .then_with(|| a.edge.edge_type.cmp(&b.edge.edge_type))
    };

    if connections.len() > top_n {
        connections.select_nth_unstable_by(top_n - 1, cmp);
        connections.truncate(top_n);
    }
    connections.sort_unstable_by(cmp);

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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_symbol(id: &str, repo: &str, name: &str) -> CodeSymbol {
        CodeSymbol {
            id: id.to_string(),
            repo_name: repo.to_string(),
            file_path: "test.rs".to_string(),
            symbol_name: name.to_string(),
            kind: "function".to_string(),
            signature: format!("fn {}()", name),
            visibility: "pub".to_string(),
            start_line: 1,
            end_line: 10,
            doc_comment: "".to_string(),
            community_id: None,
            commit_sha: "head".to_string(),
        }
    }

    fn make_test_edge(source: &str, target: &str) -> CodeEdge {
        CodeEdge {
            source_id: source.to_string(),
            target_id: target.to_string(),
            edge_type: "CALLS".to_string(),
            confidence_tier: ConfidenceTier::Extracted,
            confidence_score: 1.0,
            evidence: "".to_string(),
        }
    }

    #[test]
    fn test_brandes_betweenness_centrality_star_and_bridge_graphs() {
        // 1. Star graph: center 'hub' and 4 leaves 'l1'..'l4'
        let store = GraphStore::open_in_memory().unwrap();
        let repo_star = "repo_star";
        let hub = make_test_symbol("hub", repo_star, "hub");
        let l1 = make_test_symbol("l1", repo_star, "l1");
        let l2 = make_test_symbol("l2", repo_star, "l2");
        let l3 = make_test_symbol("l3", repo_star, "l3");
        let l4 = make_test_symbol("l4", repo_star, "l4");

        store.upsert_symbol(&hub).unwrap();
        store.upsert_symbol(&l1).unwrap();
        store.upsert_symbol(&l2).unwrap();
        store.upsert_symbol(&l3).unwrap();
        store.upsert_symbol(&l4).unwrap();

        // 4 edges connecting hub and leaves: in_degree + out_degree == 4 for hub
        // l1 -> hub, l2 -> hub, hub -> l3, hub -> l4
        store.upsert_edge(&make_test_edge("l1", "hub")).unwrap();
        store.upsert_edge(&make_test_edge("l2", "hub")).unwrap();
        store.upsert_edge(&make_test_edge("hub", "l3")).unwrap();
        store.upsert_edge(&make_test_edge("hub", "l4")).unwrap();

        let star_gods = god_nodes(&store, repo_star, 10).unwrap();
        assert_eq!(star_gods.len(), 5);

        let hub_node = star_gods.iter().find(|g| g.symbol.id == "hub").expect("hub node exists");
        assert_eq!(hub_node.in_degree + hub_node.out_degree, 4);
        assert_eq!(hub_node.total_degree, 4);
        assert!((hub_node.betweenness_centrality - 6.0).abs() < 1e-6);

        for leaf_id in &["l1", "l2", "l3", "l4"] {
            let leaf = star_gods.iter().find(|g| g.symbol.id == *leaf_id).unwrap();
            assert!((leaf.betweenness_centrality - 0.0).abs() < 1e-6);
        }

        // 2. Two-cluster bridge graph: a1-a2, a1-bridge, a2-bridge, bridge-b1, bridge-b2, b1-b2
        let repo_bridge = "repo_bridge";
        let a1 = make_test_symbol("a1", repo_bridge, "a1");
        let a2 = make_test_symbol("a2", repo_bridge, "a2");
        let bridge = make_test_symbol("bridge", repo_bridge, "bridge");
        let b1 = make_test_symbol("b1", repo_bridge, "b1");
        let b2 = make_test_symbol("b2", repo_bridge, "b2");

        store.upsert_symbol(&a1).unwrap();
        store.upsert_symbol(&a2).unwrap();
        store.upsert_symbol(&bridge).unwrap();
        store.upsert_symbol(&b1).unwrap();
        store.upsert_symbol(&b2).unwrap();

        store.upsert_edge(&make_test_edge("a1", "a2")).unwrap();
        store.upsert_edge(&make_test_edge("a1", "bridge")).unwrap();
        store.upsert_edge(&make_test_edge("a2", "bridge")).unwrap();
        store.upsert_edge(&make_test_edge("bridge", "b1")).unwrap();
        store.upsert_edge(&make_test_edge("bridge", "b2")).unwrap();
        store.upsert_edge(&make_test_edge("b1", "b2")).unwrap();

        let bridge_gods = god_nodes(&store, repo_bridge, 10).unwrap();
        assert_eq!(bridge_gods.len(), 5);

        let bridge_node = bridge_gods.iter().find(|g| g.symbol.id == "bridge").expect("bridge exists");
        assert!((bridge_node.betweenness_centrality - 4.0).abs() < 1e-6);

        for non_bridge_id in &["a1", "a2", "b1", "b2"] {
            let node = bridge_gods.iter().find(|g| g.symbol.id == *non_bridge_id).unwrap();
            assert!((node.betweenness_centrality - 0.0).abs() < 1e-6);
        }
    }

    #[test]
    fn test_bounded_top_k_god_nodes_and_surprising_connections() {
        let store = GraphStore::open_in_memory().unwrap();
        let repo = "repo_bounded";

        for i in 1..=5 {
            let mut s = make_test_symbol(&format!("s{}", i), repo, &format!("s{}", i));
            s.community_id = Some(format!("comm{}", i));
            store.upsert_symbol(&s).unwrap();
        }

        // s1 connects to s2, s3, s4, s5 (highest degree)
        // s2 connects to s3, s4
        // s3 connects to s4
        store.upsert_edge(&make_test_edge("s1", "s2")).unwrap();
        store.upsert_edge(&make_test_edge("s1", "s3")).unwrap();
        store.upsert_edge(&make_test_edge("s1", "s4")).unwrap();
        store.upsert_edge(&make_test_edge("s1", "s5")).unwrap();
        store.upsert_edge(&make_test_edge("s2", "s3")).unwrap();
        store.upsert_edge(&make_test_edge("s2", "s4")).unwrap();
        store.upsert_edge(&make_test_edge("s3", "s4")).unwrap();

        // An inferred edge to create different surprise scores
        let mut inferred_edge = make_test_edge("s4", "s5");
        inferred_edge.confidence_tier = ConfidenceTier::Inferred;
        inferred_edge.confidence_score = 0.9;
        store.upsert_edge(&inferred_edge).unwrap();

        // 1. Test god_nodes top_n == 0
        let zero_gods = god_nodes(&store, repo, 0).unwrap();
        assert!(zero_gods.is_empty());

        // 2. Test god_nodes top_n < total
        let all_gods = god_nodes(&store, repo, 10).unwrap();
        assert_eq!(all_gods.len(), 5);
        let top2_gods = god_nodes(&store, repo, 2).unwrap();
        assert_eq!(top2_gods.len(), 2);
        assert_eq!(top2_gods[0], all_gods[0]);
        assert_eq!(top2_gods[1], all_gods[1]);
        assert!(top2_gods[0].total_degree >= top2_gods[1].total_degree);

        // 3. Test surprising_connections top_n == 0
        let zero_surprises = surprising_connections(&store, repo, 0).unwrap();
        assert!(zero_surprises.is_empty());

        // 4. Test surprising_connections top_n < total
        let all_surprises = surprising_connections(&store, repo, 10).unwrap();
        assert!(all_surprises.len() >= 2);
        let top2_surprises = surprising_connections(&store, repo, 2).unwrap();
        assert_eq!(top2_surprises.len(), 2);
        assert_eq!(top2_surprises[0], all_surprises[0]);
        assert_eq!(top2_surprises[1], all_surprises[1]);
        assert!(top2_surprises[0].surprise_score >= top2_surprises[1].surprise_score);
    }
}
