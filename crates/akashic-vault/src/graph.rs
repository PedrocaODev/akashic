use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::EdgeRef;
use petgraph::Direction;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub use crate::frontmatter::scopes_overlap;
use crate::frontmatter::{
    ParsedDocument, RelationType, Scope,
};
use crate::parser::parse_markdown_file;
use crate::VaultError;

/// Edge metadata in the directed vault graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EdgeData {
    pub relation_type: RelationType,
    pub evidence: Option<String>,
    pub scope: Scope,
}

/// A resolved edge connecting two nodes in the vault.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedEdge {
    pub source: String,
    pub target: String,
    pub relation_type: RelationType,
    pub evidence: Option<String>,
    pub scope: Scope,
}

/// A relation whose target could not be resolved in the vault.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnresolvedRelation {
    pub source_id: String,
    pub source_path: String,
    pub target: String,
    pub relation_type: RelationType,
    pub evidence: Option<String>,
    pub scope: Scope,
}

/// Normalizes a path string to a canonical relative POSIX path.
pub fn normalize_posix_path(path: &str) -> Option<String> {
    let normalized = path.replace('\\', "/");
    let mut parts = Vec::new();
    for comp in normalized.split('/') {
        match comp {
            "" | "." => continue,
            ".." => {
                parts.pop()?;
            }
            c => parts.push(c),
        }
    }
    Some(parts.join("/"))
}

/// Resolves a target path relative to a source directory.
pub fn resolve_relative_path(source_dir: &str, target: &str) -> Option<String> {
    let target_norm = target.replace('\\', "/");
    if target_norm.starts_with('/') {
        return normalize_posix_path(&target_norm);
    }
    if source_dir.is_empty() {
        normalize_posix_path(&target_norm)
    } else {
        normalize_posix_path(&format!("{}/{}", source_dir, target_norm))
    }
}

/// Helper to determine whether a WalkDir entry should be skipped.
fn should_ignore_entry(entry: &walkdir::DirEntry) -> bool {
    let name = entry.file_name().to_string_lossy();
    if entry.depth() > 0 && name.starts_with('.') {
        return true;
    }
    if name == "target" || name == "node_modules" || name == ".akashic" || name == ".git" {
        return true;
    }
    false
}

/// Directed graph representation of all nodes and relations in a vault.
#[derive(Debug, Clone)]
pub struct VaultGraph {
    pub root: PathBuf,
    pub graph: DiGraph<ParsedDocument, EdgeData>,
    pub node_by_id: HashMap<String, NodeIndex>,
    pub node_by_path: HashMap<String, NodeIndex>,
    pub documents: HashMap<String, ParsedDocument>,
    pub malformed_files: Vec<(PathBuf, String)>,
    pub duplicate_ids: Vec<(String, PathBuf, PathBuf)>,
    pub unresolved_relations: Vec<UnresolvedRelation>,
}

impl VaultGraph {
    /// Creates an empty `VaultGraph` rooted at `root`.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            graph: DiGraph::new(),
            node_by_id: HashMap::new(),
            node_by_path: HashMap::new(),
            documents: HashMap::new(),
            malformed_files: Vec::new(),
            duplicate_ids: Vec::new(),
            unresolved_relations: Vec::new(),
        }
    }

    /// Recursively discovers and parses all Markdown notes under `root`, building the graph.
    pub fn from_dir(root: &Path) -> Result<Self, VaultError> {
        if !root.exists() {
            return Err(VaultError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Vault directory not found: {}", root.display()),
            )));
        }
        if !root.is_dir() {
            return Err(VaultError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("Path is not a directory: {}", root.display()),
            )));
        }

        let mut vault = Self::new(root);
        let mut parsed_nodes: Vec<(NodeIndex, ParsedDocument)> = Vec::new();

        // Pass 1: Walk files and parse markdown documents
        for entry_res in WalkDir::new(root)
            .into_iter()
            .filter_entry(|e| !should_ignore_entry(e))
        {
            let entry = match entry_res {
                Ok(e) => e,
                Err(err) => {
                    vault
                        .malformed_files
                        .push((root.to_path_buf(), err.to_string()));
                    continue;
                }
            };

            if entry.file_type().is_file() {
                let path = entry.path();
                if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                    if ext.eq_ignore_ascii_case("md") || ext.eq_ignore_ascii_case("markdown") {
                        match parse_markdown_file(root, path) {
                            Ok(doc) => {
                                let id = doc.frontmatter.id.clone();
                                let rel = doc.relative_path.clone();

                                if let Some(&existing_idx) = vault.node_by_id.get(&id) {
                                    let existing_path = vault.graph[existing_idx].path.clone();
                                    vault.duplicate_ids.push((
                                        id.clone(),
                                        existing_path,
                                        path.to_path_buf(),
                                    ));
                                }

                                vault.documents.insert(id.clone(), doc.clone());
                                let idx = vault.graph.add_node(doc.clone());
                                vault.node_by_id.insert(id, idx);
                                vault.node_by_path.insert(rel, idx);
                                parsed_nodes.push((idx, doc));
                            }
                            Err(err) => {
                                vault
                                    .malformed_files
                                    .push((path.to_path_buf(), err.to_string()));
                            }
                        }
                    }
                }
            }
        }

        // Pass 2: Resolve relations and add edges
        for (src_idx, doc) in parsed_nodes {
            let src_id = doc.frontmatter.id.clone();
            let src_rel_path = doc.relative_path.clone();
            let src_dir = Path::new(&src_rel_path)
                .parent()
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            let src_dir = if src_dir == "." { "" } else { &src_dir };

            for rel in &doc.frontmatter.relations {
                let target_raw = rel.target.trim();
                let resolved_idx = vault.resolve_target(src_dir, target_raw);

                match resolved_idx {
                    Some(tgt_idx) => {
                        vault.graph.add_edge(
                            src_idx,
                            tgt_idx,
                            EdgeData {
                                relation_type: rel.relation_type,
                                evidence: rel.evidence.clone(),
                                scope: rel.scope.clone(),
                            },
                        );
                    }
                    None => {
                        vault.unresolved_relations.push(UnresolvedRelation {
                            source_id: src_id.clone(),
                            source_path: src_rel_path.clone(),
                            target: target_raw.to_string(),
                            relation_type: rel.relation_type,
                            evidence: rel.evidence.clone(),
                            scope: rel.scope.clone(),
                        });
                    }
                }
            }
        }

        Ok(vault)
    }

    /// Resolves a target string (ID or relative path) to a NodeIndex.
    pub fn resolve_target(&self, source_dir: &str, target: &str) -> Option<NodeIndex> {
        // Direct ID lookup
        if let Some(&idx) = self.node_by_id.get(target) {
            return Some(idx);
        }

        // Lookup by relative path
        if let Some(norm_rel) = resolve_relative_path(source_dir, target) {
            if let Some(&idx) = self.node_by_path.get(&norm_rel) {
                return Some(idx);
            }
            // Try with or without .md extension
            if !norm_rel.ends_with(".md") {
                let with_md = format!("{}.md", norm_rel);
                if let Some(&idx) = self.node_by_path.get(&with_md) {
                    return Some(idx);
                }
            } else {
                let without_md = norm_rel.trim_end_matches(".md");
                if let Some(&idx) = self.node_by_id.get(without_md) {
                    return Some(idx);
                }
            }
        }

        // Direct path match in node_by_path
        let norm_target = target.replace('\\', "/");
        let clean_target = norm_target.trim_start_matches("./");
        if let Some(&idx) = self.node_by_path.get(clean_target) {
            return Some(idx);
        }

        None
    }

    /// Returns a parsed document by ID.
    pub fn get_node(&self, id: &str) -> Option<&ParsedDocument> {
        self.node_by_id.get(id).map(|&idx| &self.graph[idx])
    }

    /// Returns all outgoing edges from `node_id`.
    pub fn outgoing_edges(&self, node_id: &str) -> Vec<ResolvedEdge> {
        let Some(&idx) = self.node_by_id.get(node_id) else {
            return Vec::new();
        };

        self.graph
            .edges_directed(idx, Direction::Outgoing)
            .map(|e| ResolvedEdge {
                source: node_id.to_string(),
                target: self.graph[e.target()].frontmatter.id.clone(),
                relation_type: e.weight().relation_type,
                evidence: e.weight().evidence.clone(),
                scope: e.weight().scope.clone(),
            })
            .collect()
    }

    /// Returns all incoming edges into `node_id` (reverse backlinks).
    pub fn incoming_edges(&self, node_id: &str) -> Vec<ResolvedEdge> {
        let Some(&idx) = self.node_by_id.get(node_id) else {
            return Vec::new();
        };

        self.graph
            .edges_directed(idx, Direction::Incoming)
            .map(|e| ResolvedEdge {
                source: self.graph[e.source()].frontmatter.id.clone(),
                target: node_id.to_string(),
                relation_type: e.weight().relation_type,
                evidence: e.weight().evidence.clone(),
                scope: e.weight().scope.clone(),
            })
            .collect()
    }

    /// Finds all directed cycles composed of edges matching `allowed_types`.
    pub fn find_cycles(&self, allowed_types: &[RelationType]) -> Vec<Vec<String>> {
        let mut cycles = Vec::new();
        let mut visited = HashSet::new();
        let mut rec_stack = HashSet::new();
        let mut current_path = Vec::new();

        for node_idx in self.graph.node_indices() {
            if !visited.contains(&node_idx) {
                self.dfs_cycles(
                    node_idx,
                    allowed_types,
                    &mut visited,
                    &mut rec_stack,
                    &mut current_path,
                    &mut cycles,
                );
            }
        }

        // Canonicalize and deduplicate cycles
        let mut unique_cycles: Vec<Vec<String>> = Vec::new();
        for cycle in cycles {
            if cycle.is_empty() {
                continue;
            }
            let canonical = canonicalize_cycle(&cycle);
            if !unique_cycles.contains(&canonical) {
                unique_cycles.push(canonical);
            }
        }

        unique_cycles
    }

    fn dfs_cycles(
        &self,
        current: NodeIndex,
        allowed_types: &[RelationType],
        visited: &mut HashSet<NodeIndex>,
        rec_stack: &mut HashSet<NodeIndex>,
        path: &mut Vec<NodeIndex>,
        cycles: &mut Vec<Vec<String>>,
    ) {
        visited.insert(current);
        rec_stack.insert(current);
        path.push(current);

        for edge in self.graph.edges_directed(current, Direction::Outgoing) {
            if allowed_types.contains(&edge.weight().relation_type) {
                let target = edge.target();
                if !visited.contains(&target) {
                    self.dfs_cycles(target, allowed_types, visited, rec_stack, path, cycles);
                } else if rec_stack.contains(&target) {
                    // Cycle detected!
                    if let Some(pos) = path.iter().position(|&x| x == target) {
                        let cycle_nodes: Vec<String> = path[pos..]
                            .iter()
                            .map(|&idx| self.graph[idx].frontmatter.id.clone())
                            .collect();
                        cycles.push(cycle_nodes);
                    }
                }
            }
        }

        path.pop();
        rec_stack.remove(&current);
    }
}

/// Rotates a cycle so that the lexicographically smallest element comes first.
fn canonicalize_cycle(cycle: &[String]) -> Vec<String> {
    if cycle.is_empty() {
        return Vec::new();
    }
    let min_pos = cycle
        .iter()
        .enumerate()
        .min_by_key(|(_, id)| *id)
        .map(|(idx, _)| idx)
        .unwrap_or(0);

    let mut result = Vec::with_capacity(cycle.len());
    for i in 0..cycle.len() {
        result.push(cycle[(min_pos + i) % cycle.len()].clone());
    }
    result
}
