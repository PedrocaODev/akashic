use petgraph::graph::NodeIndex;
use petgraph::visit::EdgeRef;
use petgraph::Direction;
use std::collections::{HashSet, VecDeque};

use crate::frontmatter::{
    scopes_overlap, ConflictAlert, ContextPacket, CurrentGuidancePacket, EdgeSummary,
    HistoricalPacket, ImpactItem, ImpactPacket, LineageEntry,
    LineagePacket, NodeSummary, QueryMode, RelationType, Scope, SupersessionStep,
};
use crate::graph::VaultGraph;
use crate::VaultError;

/// Validates that a date string strictly conforms to `YYYY-MM-DD` format.
pub fn validate_date_format(date_str: &str) -> Result<(), VaultError> {
    let bytes = date_str.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return Err(VaultError::Conflict(format!(
            "Invalid date format '{}'. Expected YYYY-MM-DD",
            date_str
        )));
    }

    let year = &date_str[0..4];
    let month = &date_str[5..7];
    let day = &date_str[8..10];

    if !year.chars().all(|c| c.is_ascii_digit())
        || !month.chars().all(|c| c.is_ascii_digit())
        || !day.chars().all(|c| c.is_ascii_digit())
    {
        return Err(VaultError::Conflict(format!(
            "Invalid date format '{}'. Expected YYYY-MM-DD",
            date_str
        )));
    }

    let m: u32 = month
        .parse()
        .map_err(|_| VaultError::Conflict(format!("Invalid month in date '{}'", date_str)))?;
    let d: u32 = day
        .parse()
        .map_err(|_| VaultError::Conflict(format!("Invalid day in date '{}'", date_str)))?;

    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return Err(VaultError::Conflict(format!(
            "Date value out of range '{}'",
            date_str
        )));
    }

    Ok(())
}

/// Determines whether candidate `idx_b` transitively supersedes candidate `idx_a`
/// along a directed path of `supersedes` edges with pairwise overlapping scopes.
pub fn transitively_supersedes(
    vault: &VaultGraph,
    idx_b: NodeIndex,
    idx_a: NodeIndex,
    scope: &Scope,
) -> bool {
    if idx_b == idx_a {
        return false;
    }

    let mut queue = VecDeque::new();
    let mut visited = HashSet::new();

    queue.push_back((idx_b, None::<Scope>));
    visited.insert(idx_b);

    while let Some((curr_idx, prev_scope)) = queue.pop_front() {
        for edge in vault.graph.edges_directed(curr_idx, Direction::Outgoing) {
            if edge.weight().relation_type == RelationType::Supersedes {
                let target_idx = edge.target();
                let edge_sc = if edge.weight().scope.is_empty() {
                    &vault.graph[curr_idx].frontmatter.scope
                } else {
                    &edge.weight().scope
                };

                let pairwise = match prev_scope {
                    Some(ref ps) => scopes_overlap(edge_sc, ps),
                    None => true,
                };

                if pairwise && scopes_overlap(edge_sc, scope) {
                    if target_idx == idx_a {
                        return true;
                    }
                    if visited.insert(target_idx) {
                        queue.push_back((target_idx, Some(edge_sc.clone())));
                    }
                }
            }
        }
    }

    false
}

/// Executes a universal graph query according to the requested mode.
pub fn query(
    graph: &VaultGraph,
    mode: QueryMode,
    seed_id: &str,
    scope: Option<&Scope>,
    as_of: Option<&str>,
) -> Result<ContextPacket, VaultError> {
    match mode {
        QueryMode::Current => query_current(graph, seed_id, scope),
        QueryMode::Lineage => query_lineage(graph, seed_id, scope),
        QueryMode::Impact => query_impact(graph, seed_id, scope),
        QueryMode::Historical => query_historical(graph, seed_id, scope, as_of),
    }
}

/// Resolves current active guidance with strict conflict detection.
pub fn query_current(
    graph: &VaultGraph,
    seed_id: &str,
    scope: Option<&Scope>,
) -> Result<ContextPacket, VaultError> {
    let empty_scope = Scope::new();
    let query_scope = scope.unwrap_or(&empty_scope);

    let seed_idx = graph
        .node_by_id
        .get(seed_id)
        .copied()
        .ok_or_else(|| VaultError::Conflict(format!("Seed note '{}' not found", seed_id)))?;

    // 1. Check for supersession cycles involving the seed or its successors
    let cycles = graph.find_cycles(&[RelationType::Supersedes]);
    for cycle in &cycles {
        let cycle_set: HashSet<&str> = cycle.iter().map(|s| s.as_str()).collect();
        // Check if seed is in cycle
        if cycle_set.contains(seed_id) {
            let alert = ConflictAlert {
                alert: "[UNRESOLVED_CONFLICT]".to_string(),
                kind: "supersession_cycle".to_string(),
                message: format!(
                    "Supersession cycle detected involving seed note '{}': {:?}",
                    seed_id, cycle
                ),
                predecessor: None,
                cycle: Some(cycle.clone()),
                candidates: cycle.clone(),
            };

            let focal_nodes: Vec<NodeSummary> = cycle
                .iter()
                .filter_map(|id| graph.documents.get(id).map(NodeSummary::from))
                .collect();

            let current_packet = CurrentGuidancePacket {
                mode: "current".to_string(),
                seed: seed_id.to_string(),
                active_guidance_id: None,
                is_current: false,
                status: "superseded".to_string(),
                query_scope: query_scope.clone(),
                active_node: None,
                unresolved_conflict: Some(alert.clone()),
                supersession_chain: Vec::new(),
                historical_rationale: Vec::new(),
                advisories: vec![format!("Supersession cycle: {:?}", cycle)],
                message: alert.message.clone(),
            };

            return Ok(ContextPacket {
                mode: QueryMode::Current,
                seed_id: seed_id.to_string(),
                scope_filter: query_scope.clone(),
                as_of: None,
                abstained: true,
                focal_nodes,
                ancestors: Vec::new(),
                descendants: Vec::new(),
                supporting_evidence: Vec::new(),
                contradictions: Vec::new(),
                unresolved_conflict: Some(alert.clone()),
                unresolved_conflicts: vec![alert],
                edges: Vec::new(),
                warnings: vec!["Supersession cycle detected".to_string()],
                current_packet: Some(current_packet),
                lineage_packet: None,
                impact_packet: None,
                historical_packet: None,
            });
        }
    }

    // 2. Discover all active successors that supersede the seed (incoming supersedes edges)
    // In our forward model: successor --supersedes--> predecessor.
    // So successors are reachable by following incoming supersedes edges.
    let mut successor_indices = Vec::new();
    let mut queue = VecDeque::new();
    let mut visited = HashSet::new();

    queue.push_back(seed_idx);
    visited.insert(seed_idx);

    while let Some(curr_idx) = queue.pop_front() {
        for edge in graph.graph.edges_directed(curr_idx, Direction::Incoming) {
            if edge.weight().relation_type == RelationType::Supersedes {
                let succ_idx = edge.source();
                let edge_sc = if edge.weight().scope.is_empty() {
                    &graph.graph[succ_idx].frontmatter.scope
                } else {
                    &edge.weight().scope
                };

                if scopes_overlap(edge_sc, query_scope) && visited.insert(succ_idx) {
                    successor_indices.push(succ_idx);
                    queue.push_back(succ_idx);
                }
            }
        }
    }

    // Filter to active candidates
    let mut active_candidates: Vec<NodeIndex> = successor_indices
        .into_iter()
        .filter(|&idx| {
            let doc = &graph.graph[idx];
            doc.frontmatter.status.is_active()
                && scopes_overlap(&doc.frontmatter.scope, query_scope)
        })
        .collect();

    // If no active successors were found, check if the seed itself is active
    let seed_doc = &graph.graph[seed_idx];
    if active_candidates.is_empty()
        && seed_doc.frontmatter.status.is_active()
        && scopes_overlap(&seed_doc.frontmatter.scope, query_scope)
    {
        active_candidates.push(seed_idx);
    }

    // Filter out candidates that are transitively superseded by another candidate
    let mut maximal_candidates = Vec::new();
    for &cand in &active_candidates {
        let is_superseded = active_candidates.iter().any(|&other| {
            other != cand && transitively_supersedes(graph, other, cand, query_scope)
        });
        if !is_superseded {
            maximal_candidates.push(cand);
        }
    }

    // Check for competing successors conflict
    if maximal_candidates.len() > 1 {
        // Check if any two maximal candidates have overlapping scopes
        let has_scope_conflict = maximal_candidates.iter().enumerate().any(|(i, &c1)| {
            maximal_candidates.iter().skip(i + 1).any(|&c2| {
                scopes_overlap(
                    &graph.graph[c1].frontmatter.scope,
                    &graph.graph[c2].frontmatter.scope,
                )
            })
        });

        if has_scope_conflict {
            let candidate_ids: Vec<String> = maximal_candidates
                .iter()
                .map(|&idx| graph.graph[idx].frontmatter.id.clone())
                .collect();

            let alert = ConflictAlert {
                alert: "[UNRESOLVED_CONFLICT]".to_string(),
                kind: "competing_successors".to_string(),
                message: format!(
                    "Multiple active competing successors supersede '{}' in overlapping scopes: {:?}",
                    seed_id, candidate_ids
                ),
                predecessor: Some(seed_id.to_string()),
                cycle: None,
                candidates: candidate_ids.clone(),
            };

            let focal_nodes: Vec<NodeSummary> = maximal_candidates
                .iter()
                .map(|&idx| NodeSummary::from(&graph.graph[idx]))
                .collect();

            let current_packet = CurrentGuidancePacket {
                mode: "current".to_string(),
                seed: seed_id.to_string(),
                active_guidance_id: None,
                is_current: false,
                status: "superseded".to_string(),
                query_scope: query_scope.clone(),
                active_node: None,
                unresolved_conflict: Some(alert.clone()),
                supersession_chain: Vec::new(),
                historical_rationale: Vec::new(),
                advisories: vec![format!("Competing successors: {:?}", candidate_ids)],
                message: alert.message.clone(),
            };

            return Ok(ContextPacket {
                mode: QueryMode::Current,
                seed_id: seed_id.to_string(),
                scope_filter: query_scope.clone(),
                as_of: None,
                abstained: true,
                focal_nodes,
                ancestors: Vec::new(),
                descendants: Vec::new(),
                supporting_evidence: Vec::new(),
                contradictions: Vec::new(),
                unresolved_conflict: Some(alert.clone()),
                unresolved_conflicts: vec![alert],
                edges: Vec::new(),
                warnings: vec!["Competing active successors detected".to_string()],
                current_packet: Some(current_packet),
                lineage_packet: None,
                impact_packet: None,
                historical_packet: None,
            });
        }
    }

    // Exactly one active winner or none
    let winner_idx = maximal_candidates.first().copied();

    let (focal_nodes, active_node, active_id) = match winner_idx {
        Some(w_idx) => {
            let doc = &graph.graph[w_idx];
            let summary = NodeSummary::from(doc);
            (
                vec![summary.clone()],
                Some(summary),
                Some(doc.frontmatter.id.clone()),
            )
        }
        None => (Vec::new(), None, None),
    };

    // Build supersession chain and ancestors
    let mut supersession_chain = Vec::new();
    let mut ancestors = Vec::new();

    if let Some(w_idx) = winner_idx {
        if w_idx != seed_idx {
            // Traverse from winner down to seed
            let mut curr = w_idx;
            while curr != seed_idx {
                let mut found_next = None;
                for edge in graph.graph.edges_directed(curr, Direction::Outgoing) {
                    if edge.weight().relation_type == RelationType::Supersedes {
                        let target = edge.target();
                        supersession_chain.push(SupersessionStep {
                            predecessor: graph.graph[target].frontmatter.id.clone(),
                            successor: graph.graph[curr].frontmatter.id.clone(),
                            evidence: edge.weight().evidence.clone(),
                            scope: edge.weight().scope.clone(),
                        });
                        ancestors.push(NodeSummary::from(&graph.graph[target]));
                        found_next = Some(target);
                        break;
                    }
                }
                match found_next {
                    Some(n) => curr = n,
                    None => break,
                }
            }
        }
    }

    let is_current = active_id.as_deref() == Some(seed_id);
    let status_str = seed_doc.frontmatter.status.as_str().to_string();

    let current_packet = CurrentGuidancePacket {
        mode: "current".to_string(),
        seed: seed_id.to_string(),
        active_guidance_id: active_id,
        is_current,
        status: status_str,
        query_scope: query_scope.clone(),
        active_node,
        unresolved_conflict: None,
        supersession_chain,
        historical_rationale: Vec::new(),
        advisories: Vec::new(),
        message: if is_current {
            format!("Seed '{}' is currently active", seed_id)
        } else {
            format!("Resolved active guidance for seed '{}'", seed_id)
        },
    };

    Ok(ContextPacket {
        mode: QueryMode::Current,
        seed_id: seed_id.to_string(),
        scope_filter: query_scope.clone(),
        as_of: None,
        abstained: false,
        focal_nodes,
        ancestors,
        descendants: Vec::new(),
        supporting_evidence: Vec::new(),
        contradictions: Vec::new(),
        unresolved_conflict: None,
        unresolved_conflicts: Vec::new(),
        edges: Vec::new(),
        warnings: Vec::new(),
        current_packet: Some(current_packet),
        lineage_packet: None,
        impact_packet: None,
        historical_packet: None,
    })
}

/// Traces the full supersession and causality lineage for a seed note.
pub fn query_lineage(
    graph: &VaultGraph,
    seed_id: &str,
    scope: Option<&Scope>,
) -> Result<ContextPacket, VaultError> {
    let empty_scope = Scope::new();
    let query_scope = scope.unwrap_or(&empty_scope);

    let seed_idx = graph
        .node_by_id
        .get(seed_id)
        .copied()
        .ok_or_else(|| VaultError::Conflict(format!("Seed note '{}' not found", seed_id)))?;

    let seed_doc = &graph.graph[seed_idx];
    let mut chain = Vec::new();
    let mut transitions = Vec::new();
    let mut ancestors = Vec::new();
    let mut descendants = Vec::new();
    let mut evidence_nodes = Vec::new();
    let mut edges = Vec::new();

    // 1. Trace supersedes predecessors (outgoing supersedes edges)
    let mut curr = seed_idx;
    let mut visited_pred = HashSet::new();
    visited_pred.insert(curr);

    while let Some(edge) = graph
        .graph
        .edges_directed(curr, Direction::Outgoing)
        .find(|e| e.weight().relation_type == RelationType::Supersedes)
    {
        let target_idx = edge.target();
        if !visited_pred.insert(target_idx) {
            break; // Avoid cycles
        }
        let target_doc = &graph.graph[target_idx];
        ancestors.push(NodeSummary::from(target_doc));
        transitions.push(SupersessionStep {
            predecessor: target_doc.frontmatter.id.clone(),
            successor: graph.graph[curr].frontmatter.id.clone(),
            evidence: edge.weight().evidence.clone(),
            scope: edge.weight().scope.clone(),
        });
        edges.push(EdgeSummary {
            source: graph.graph[curr].frontmatter.id.clone(),
            target: target_doc.frontmatter.id.clone(),
            relation_type: RelationType::Supersedes,
            evidence: edge.weight().evidence.clone(),
            scope: edge.weight().scope.clone(),
        });
        curr = target_idx;
    }

    // 2. Trace supersedes successors (incoming supersedes edges)
    let mut curr_succ = seed_idx;
    let mut visited_succ = HashSet::new();
    visited_succ.insert(curr_succ);

    while let Some(edge) = graph
        .graph
        .edges_directed(curr_succ, Direction::Incoming)
        .find(|e| e.weight().relation_type == RelationType::Supersedes)
    {
        let src_idx = edge.source();
        if !visited_succ.insert(src_idx) {
            break;
        }
        let src_doc = &graph.graph[src_idx];
        descendants.push(NodeSummary::from(src_doc));
        transitions.push(SupersessionStep {
            predecessor: graph.graph[curr_succ].frontmatter.id.clone(),
            successor: src_doc.frontmatter.id.clone(),
            evidence: edge.weight().evidence.clone(),
            scope: edge.weight().scope.clone(),
        });
        edges.push(EdgeSummary {
            source: src_doc.frontmatter.id.clone(),
            target: graph.graph[curr_succ].frontmatter.id.clone(),
            relation_type: RelationType::Supersedes,
            evidence: edge.weight().evidence.clone(),
            scope: edge.weight().scope.clone(),
        });
        curr_succ = src_idx;
    }

    // 3. Trace caused_by and resolves relations
    for edge in graph.graph.edges_directed(seed_idx, Direction::Outgoing) {
        let rt = edge.weight().relation_type;
        if rt == RelationType::CausedBy || rt == RelationType::Resolves || rt == RelationType::SupportedBy {
            let tgt_idx = edge.target();
            evidence_nodes.push(NodeSummary::from(&graph.graph[tgt_idx]));
            edges.push(EdgeSummary {
                source: seed_id.to_string(),
                target: graph.graph[tgt_idx].frontmatter.id.clone(),
                relation_type: rt,
                evidence: edge.weight().evidence.clone(),
                scope: edge.weight().scope.clone(),
            });
        }
    }

    let focal_summary = NodeSummary::from(seed_doc);
    chain.push(LineageEntry {
        node: focal_summary.clone(),
        supersedes: None,
        superseded_by: None,
        evidence: None,
        is_current: seed_doc.frontmatter.status.is_active(),
    });

    let lineage_packet = LineagePacket {
        mode: "lineage".to_string(),
        seed: seed_id.to_string(),
        active_guidance_id: Some(seed_id.to_string()),
        query_scope: query_scope.clone(),
        chain,
        transitions,
        empirical_evidence: evidence_nodes.clone(),
        advisories: Vec::new(),
        message: format!("Traced lineage for seed '{}'", seed_id),
    };

    Ok(ContextPacket {
        mode: QueryMode::Lineage,
        seed_id: seed_id.to_string(),
        scope_filter: query_scope.clone(),
        as_of: None,
        abstained: false,
        focal_nodes: vec![focal_summary],
        ancestors,
        descendants,
        supporting_evidence: evidence_nodes,
        contradictions: Vec::new(),
        unresolved_conflict: None,
        unresolved_conflicts: Vec::new(),
        edges,
        warnings: Vec::new(),
        current_packet: None,
        lineage_packet: Some(lineage_packet),
        impact_packet: None,
        historical_packet: None,
    })
}

/// Performs blast radius impact analysis by traversing reverse dependency edges.
pub fn query_impact(
    graph: &VaultGraph,
    seed_id: &str,
    scope: Option<&Scope>,
) -> Result<ContextPacket, VaultError> {
    let empty_scope = Scope::new();
    let query_scope = scope.unwrap_or(&empty_scope);

    let seed_idx = graph
        .node_by_id
        .get(seed_id)
        .copied()
        .ok_or_else(|| VaultError::Conflict(format!("Seed note '{}' not found", seed_id)))?;

    let seed_doc = &graph.graph[seed_idx];
    let mut dependents = Vec::new();
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    let mut edges = Vec::new();

    visited.insert(seed_idx);
    queue.push_back((seed_idx, 0, vec![seed_id.to_string()]));

    let impact_types = [
        RelationType::DependsOn,
        RelationType::SupportedBy,
        RelationType::CausedBy,
        RelationType::Substantiates,
        RelationType::Resolves,
    ];

    let mut max_depth = 0;

    while let Some((curr_idx, depth, path)) = queue.pop_front() {
        if depth > max_depth {
            max_depth = depth;
        }

        // Incoming edges whose type is in impact_types (nodes that depend on curr)
        for edge in graph.graph.edges_directed(curr_idx, Direction::Incoming) {
            let rt = edge.weight().relation_type;
            if impact_types.contains(&rt) {
                let dep_idx = edge.source();
                let dep_doc = &graph.graph[dep_idx];

                if scopes_overlap(&dep_doc.frontmatter.scope, query_scope) {
                    edges.push(EdgeSummary {
                        source: dep_doc.frontmatter.id.clone(),
                        target: graph.graph[curr_idx].frontmatter.id.clone(),
                        relation_type: rt,
                        evidence: edge.weight().evidence.clone(),
                        scope: edge.weight().scope.clone(),
                    });

                    if visited.insert(dep_idx) {
                        let mut new_path = path.clone();
                        new_path.push(dep_doc.frontmatter.id.clone());

                        dependents.push(ImpactItem {
                            id: dep_doc.frontmatter.id.clone(),
                            title: dep_doc.frontmatter.title.clone(),
                            node_type: dep_doc.frontmatter.node_type.clone(),
                            depth: depth + 1,
                            path: new_path.clone(),
                            scope: dep_doc.frontmatter.scope.clone(),
                            file_path: Some(dep_doc.relative_path.clone()),
                        });

                        queue.push_back((dep_idx, depth + 1, new_path));
                    }
                }
            }
        }
    }

    let impact_packet = ImpactPacket {
        mode: "impact".to_string(),
        seed: seed_id.to_string(),
        seed_title: seed_doc.frontmatter.title.clone(),
        query_scope: query_scope.clone(),
        total_dependents: dependents.len(),
        max_depth,
        dependents: dependents.clone(),
        message: format!(
            "Impact analysis for '{}': {} dependents found (max depth: {})",
            seed_id,
            dependents.len(),
            max_depth
        ),
    };

    let focal_summary = NodeSummary::from(seed_doc);
    let descendant_summaries: Vec<NodeSummary> = dependents
        .iter()
        .filter_map(|d| graph.documents.get(&d.id).map(NodeSummary::from))
        .collect();

    Ok(ContextPacket {
        mode: QueryMode::Impact,
        seed_id: seed_id.to_string(),
        scope_filter: query_scope.clone(),
        as_of: None,
        abstained: false,
        focal_nodes: vec![focal_summary],
        ancestors: Vec::new(),
        descendants: descendant_summaries,
        supporting_evidence: Vec::new(),
        contradictions: Vec::new(),
        unresolved_conflict: None,
        unresolved_conflicts: Vec::new(),
        edges,
        warnings: Vec::new(),
        current_packet: None,
        lineage_packet: None,
        impact_packet: Some(impact_packet),
        historical_packet: None,
    })
}

/// Queries point-in-time active guidance evaluated as of a specific date.
pub fn query_historical(
    graph: &VaultGraph,
    seed_id: &str,
    scope: Option<&Scope>,
    as_of: Option<&str>,
) -> Result<ContextPacket, VaultError> {
    let empty_scope = Scope::new();
    let query_scope = scope.unwrap_or(&empty_scope);

    let as_of_date = as_of.ok_or_else(|| {
        VaultError::Conflict("Missing required as_of date parameter for historical query".to_string())
    })?;
    validate_date_format(as_of_date)?;

    let seed_idx = graph
        .node_by_id
        .get(seed_id)
        .copied()
        .ok_or_else(|| VaultError::Conflict(format!("Seed note '{}' not found", seed_id)))?;

    // Collect seed and all reachable nodes in its supersession lineage
    let mut lineage_nodes = Vec::new();
    lineage_nodes.push(seed_idx);

    // Upstream predecessors
    let mut curr = seed_idx;
    while let Some(edge) = graph
        .graph
        .edges_directed(curr, Direction::Outgoing)
        .find(|e| e.weight().relation_type == RelationType::Supersedes)
    {
        let tgt = edge.target();
        lineage_nodes.push(tgt);
        curr = tgt;
    }

    // Downstream successors
    let mut curr_succ = seed_idx;
    while let Some(edge) = graph
        .graph
        .edges_directed(curr_succ, Direction::Incoming)
        .find(|e| e.weight().relation_type == RelationType::Supersedes)
    {
        let src = edge.source();
        lineage_nodes.push(src);
        curr_succ = src;
    }

    // Check which node in the lineage was valid as of date
    let is_valid_as_of = |doc: &crate::frontmatter::ParsedDocument| -> bool {
        if !scopes_overlap(&doc.frontmatter.scope, query_scope) {
            return false;
        }
        let from_ok = match &doc.frontmatter.valid_from {
            Some(vf) => vf.as_str() <= as_of_date,
            None => true,
        };
        let until_ok = match &doc.frontmatter.valid_until {
            Some(vu) => as_of_date < vu.as_str(),
            None => true,
        };
        from_ok && until_ok
    };

    let active_node_idx = lineage_nodes.iter().copied().find(|&idx| is_valid_as_of(&graph.graph[idx]));

    let (active_summary, active_id, is_active) = match active_node_idx {
        Some(idx) => {
            let doc = &graph.graph[idx];
            let summary = NodeSummary::from(doc);
            let id = doc.frontmatter.id.clone();
            let is_seed = id == seed_id;
            (Some(summary), Some(id), is_seed)
        }
        None => (None, None, false),
    };

    let historical_packet = HistoricalPacket {
        mode: "historical".to_string(),
        seed: seed_id.to_string(),
        as_of: as_of_date.to_string(),
        active_guidance_id: active_id,
        is_active,
        status: if is_active { "active".to_string() } else { "inactive".to_string() },
        query_scope: query_scope.clone(),
        active_node: active_summary.clone(),
        warnings: Vec::new(),
        advisories: Vec::new(),
        message: format!("Historical status as of {}: {}", as_of_date, if is_active { "active" } else { "inactive" }),
    };

    let focal_nodes = active_summary.into_iter().collect();

    Ok(ContextPacket {
        mode: QueryMode::Historical,
        seed_id: seed_id.to_string(),
        scope_filter: query_scope.clone(),
        as_of: Some(as_of_date.to_string()),
        abstained: false,
        focal_nodes,
        ancestors: Vec::new(),
        descendants: Vec::new(),
        supporting_evidence: Vec::new(),
        contradictions: Vec::new(),
        unresolved_conflict: None,
        unresolved_conflicts: Vec::new(),
        edges: Vec::new(),
        warnings: Vec::new(),
        current_packet: None,
        lineage_packet: None,
        impact_packet: None,
        historical_packet: Some(historical_packet),
    })
}
