use std::collections::BTreeMap;
use std::path::PathBuf;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock};
use rmcp::tool;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use akashic_engine::store::GraphStore;
use akashic_rca::pipeline::{ConductorPipeline, PipelineInput};
use akashic_rca::stage4::{CandidateFinding, FindingOutcome, MockStage4Executor};
use akashic_rca::store::StateStore;
use akashic_rca::symbolicator::{
    symbolicate_crash, EngineSymbolResolver, InMemorySymbolResolver, SymbolResolver,
};
use akashic_rca::CurationVerdict;
use akashic_vault::{
    query::query, query::query_current, query::query_lineage,
    serialize_document, Frontmatter, NodeStatus, NodeType, QueryMode, Scope,
    VaultGraph,
};

#[derive(Clone, Debug)]
pub struct AkashicMcpServer {
    pub vault_path: PathBuf,
    pub workspace_path: PathBuf,
    pub config: Option<crate::AkashicConfig>,
}

impl AkashicMcpServer {
    pub fn new(vault_path: PathBuf, workspace_path: PathBuf) -> Self {
        Self {
            vault_path,
            workspace_path,
            config: None,
        }
    }

    pub fn with_config(vault_path: PathBuf, workspace_path: PathBuf, config: crate::AkashicConfig) -> Self {
        Self {
            vault_path,
            workspace_path,
            config: Some(config),
        }
    }

    pub fn graph_db_path(&self) -> PathBuf {
        if let Some(ref cfg) = self.config {
            let p = PathBuf::from(&cfg.engine.db_path);
            if p.is_absolute() || p.exists() {
                return p;
            }
            let candidate = self.workspace_path.join(&p);
            if candidate.exists() {
                return candidate;
            }
            return p;
        }
        let candidate = self.workspace_path.join(".akashic/code_graph.db");
        if candidate.exists() {
            candidate
        } else if PathBuf::from(".akashic/code_graph.db").exists() {
            PathBuf::from(".akashic/code_graph.db")
        } else {
            candidate
        }
    }

    pub fn rca_db_path(&self) -> PathBuf {
        if let Some(ref cfg) = self.config {
            let p = PathBuf::from(&cfg.rca.db_path);
            if p.is_absolute() || p.exists() {
                return p;
            }
            let candidate = self.workspace_path.join(&p);
            if candidate.exists() {
                return candidate;
            }
            return p;
        }
        let candidate = self.workspace_path.join(".akashic/rca_state.db");
        if candidate.exists() {
            candidate
        } else if PathBuf::from(".akashic/rca_state.db").exists() {
            PathBuf::from(".akashic/rca_state.db")
        } else {
            candidate
        }
    }
}

fn tool_ok<T: serde::Serialize>(val: &T) -> CallToolResult {
    match serde_json::to_string_pretty(val) {
        Ok(s) => CallToolResult::success(vec![ContentBlock::text(s)]),
        Err(e) => CallToolResult::error(vec![ContentBlock::text(format!("Serialization error: {}", e))]),
    }
}

fn tool_text(s: impl Into<String>) -> CallToolResult {
    CallToolResult::success(vec![ContentBlock::text(s.into())])
}

fn tool_err(s: impl Into<String>) -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text(s.into())])
}

fn parse_scope(scope_str: Option<&str>) -> Scope {
    let mut scope = Scope::new();
    if let Some(raw) = scope_str {
        if let Ok(map) = serde_json::from_str::<BTreeMap<String, String>>(raw) {
            return map;
        }
        for part in raw.split(',') {
            let part = part.trim();
            if let Some((k, v)) = part.split_once('=') {
                scope.insert(k.trim().to_string(), v.trim().to_string());
            }
        }
    }
    scope
}

// ---------------------------------------------------------------------------
// Tool parameter schemas
// ---------------------------------------------------------------------------

fn normalize_tool_schema(schema: &mut schemars::Schema) {
    if let Some(obj) = schema.as_object_mut() {
        if let Some(serde_json::Value::Array(types)) = obj.get_mut("type") {
            types.retain(|t| t.as_str() != Some("null"));
            if types.len() == 1 {
                let single = types.remove(0);
                obj.insert("type".to_string(), single);
            }
        }
        if obj.get("default").is_some_and(|v| v.is_null()) {
            obj.remove("default");
        }
        if obj
            .get("format")
            .and_then(|v| v.as_str())
            .is_some_and(|f| matches!(f, "uint" | "uint32" | "uint64"))
        {
            obj.remove("format");
        }
    }
    schemars::transform::transform_subschemas(&mut normalize_tool_schema, schema);
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(transform = normalize_tool_schema)]
pub struct VaultQueryParams {
    pub mode: String,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default, alias = "note_id")]
    pub node_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(transform = normalize_tool_schema)]
pub struct VaultInspectParams {
    #[serde(alias = "note_id")]
    pub node_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(transform = normalize_tool_schema)]
pub struct VaultLintParams {
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub strict: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(transform = normalize_tool_schema)]
pub struct VaultReadParams {
    #[serde(alias = "note_id")]
    pub node_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(transform = normalize_tool_schema)]
pub struct VaultWriteCanonicalParams {
    #[serde(alias = "note_id")]
    pub node_id: String,
    pub title: String,
    pub node_type: String,
    pub content: String,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(transform = normalize_tool_schema)]
pub struct GraphQueryParams {
    pub query: String,
    #[serde(default)]
    pub repo_name: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(transform = normalize_tool_schema)]
pub struct GraphGetNodeParams {
    pub node_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(transform = normalize_tool_schema)]
pub struct GraphGetNeighborsParams {
    pub node_id: String,
    #[serde(default)]
    pub direction: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(transform = normalize_tool_schema)]
pub struct GraphGetCommunityParams {
    #[serde(default)]
    pub community_id: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(transform = normalize_tool_schema)]
pub struct GraphGodNodesParams {
    #[serde(default)]
    pub repo_name: Option<String>,
    #[serde(default)]
    pub top_n: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(transform = normalize_tool_schema)]
pub struct GraphStatsParams {
    #[serde(default)]
    pub repo_name: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(transform = normalize_tool_schema)]
pub struct GraphShortestPathParams {
    pub source: String,
    pub target: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(transform = normalize_tool_schema)]
pub struct GraphBlastRadiusParams {
    pub target: String,
    #[serde(default)]
    pub max_depth: Option<usize>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[schemars(transform = normalize_tool_schema)]
pub struct RcaStatusParams {
    pub investigation_id: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[schemars(transform = normalize_tool_schema)]
pub struct RcaTriggerParams {
    pub app_id: String,
    pub raw_log: String,
    #[serde(default)]
    pub two_phase: Option<bool>,
    #[serde(default)]
    pub jira_key: Option<String>,
    #[serde(default)]
    pub run_id: Option<String>,
    #[serde(default)]
    pub prior_resolution_id: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[schemars(transform = normalize_tool_schema)]
pub struct RcaSymbolicateParams {
    pub app_id: String,
    pub raw_log: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[schemars(transform = normalize_tool_schema)]
pub struct RcaCrystallizeParams {
    pub run_id: String,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub winning_candidate_id: Option<String>,
    #[serde(default)]
    pub candidate_id: Option<String>,
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub outcome: Option<String>,
    #[serde(default)]
    pub primary_hypothesis: Option<String>,
    #[serde(default)]
    pub falsification_attempts: Option<Vec<String>>,
    #[serde(default)]
    pub invalidated_hypotheses: Option<Vec<String>>,
    #[serde(default)]
    pub failed_attempts: Option<Vec<String>>,
    #[serde(default)]
    pub confidence_score: Option<f64>,
    #[serde(default)]
    pub remediation_proposal: Option<String>,
    #[serde(default)]
    pub remediation_type: Option<String>,
    #[serde(default)]
    pub hold_for_curation: Option<bool>,
    #[serde(default)]
    pub finding_file: Option<String>,
}

// ---------------------------------------------------------------------------
// Tool implementations
// ---------------------------------------------------------------------------

#[rmcp::tool_router(server_handler)]
impl AkashicMcpServer {
    // 1. Vault Query
    #[tool(name = "vault_query", description = "Query vault nodes by mode (current, lineage, impact, historical) and optional scope filter")]
    pub async fn vault_query(&self, Parameters(params): Parameters<VaultQueryParams>) -> CallToolResult {
        if !self.vault_path.exists() {
            return tool_err(format!("Vault path '{}' does not exist", self.vault_path.display()));
        }
        let graph = match VaultGraph::from_dir(&self.vault_path) {
            Ok(g) => g,
            Err(e) => return tool_err(format!("Failed to load vault graph: {}", e)),
        };

        let scope = parse_scope(params.scope.as_deref());
        let seed_id = match params.node_id.as_deref() {
            Some(id) => {
                if graph.get_node(id).is_none() {
                    return tool_err(format!("Vault Node '{}' not found", id));
                }
                id.to_string()
            }
            None => {
                match graph.node_by_id.keys().next() {
                    Some(first) => first.clone(),
                    None => return tool_text("{\"focal_nodes\":[],\"message\":\"Vault is empty\"}"),
                }
            }
        };

        let res = match params.mode.to_ascii_lowercase().as_str() {
            "current" => query_current(&graph, &seed_id, Some(&scope)),
            "lineage" => query_lineage(&graph, &seed_id, Some(&scope)),
            "impact" => query(&graph, QueryMode::Impact, &seed_id, Some(&scope), None),
            "historical" => query(&graph, QueryMode::Historical, &seed_id, Some(&scope), None),
            other => return tool_err(format!("Unsupported query mode '{}'", other)),
        };

        match res {
            Ok(packet) => tool_ok(&packet),
            Err(e) => tool_err(format!("Vault query failed: {}", e)),
        }
    }

    // 2. Vault Inspect
    #[tool(name = "vault_inspect", description = "Inspect a vault note by ID to view metadata, frontmatter, and relations")]
    pub async fn vault_inspect(&self, Parameters(params): Parameters<VaultInspectParams>) -> CallToolResult {
        if !self.vault_path.exists() {
            return tool_err(format!("Vault path '{}' does not exist", self.vault_path.display()));
        }
        let graph = match VaultGraph::from_dir(&self.vault_path) {
            Ok(g) => g,
            Err(e) => return tool_err(format!("Failed to load vault graph: {}", e)),
        };

        match graph.get_node(&params.node_id) {
            Some(vault_node) => {
                let outgoing = graph.outgoing_edges(&params.node_id);
                let incoming = graph.incoming_edges(&params.node_id);
                let info = serde_json::json!({
                    "id": vault_node.frontmatter.id,
                    "title": vault_node.frontmatter.title,
                    "type": vault_node.frontmatter.node_type,
                    "status": vault_node.frontmatter.status,
                    "scope": vault_node.frontmatter.scope,
                    "tags": vault_node.frontmatter.tags,
                    "relative_path": vault_node.relative_path,
                    "outgoing_edges": outgoing,
                    "incoming_edges": incoming,
                });
                tool_ok(&info)
            }
            None => tool_err(format!("Vault Node '{}' not found", params.node_id)),
        }
    }

    // 3. Vault Lint
    #[tool(name = "vault_lint", description = "Lint vault directory for Google OKF v0.2 rule compliance")]
    pub async fn vault_lint(&self, Parameters(params): Parameters<VaultLintParams>) -> CallToolResult {
        let target = params
            .path
            .map(PathBuf::from)
            .unwrap_or_else(|| self.vault_path.clone());
        if !target.exists() {
            return tool_err(format!("Lint target path '{}' does not exist", target.display()));
        }
        let mut config = self.config.as_ref().map(|c| c.lint.clone()).unwrap_or_default();
        if let Some(strict) = params.strict {
            config.strict_forward_only = strict;
        }

        let graph = match VaultGraph::from_dir(&target) {
            Ok(g) => g,
            Err(e) => return tool_err(format!("Failed to parse vault graph for linting: {}", e)),
        };
        let report = akashic_lint::lint_vault_with_config(&graph, &config);
        tool_ok(&report)
    }

    // 4. Vault Read
    #[tool(name = "vault_read", description = "Read markdown contents of a vault note by ID")]
    pub async fn vault_read(&self, Parameters(params): Parameters<VaultReadParams>) -> CallToolResult {
        if !self.vault_path.exists() {
            return tool_err(format!("Vault path '{}' does not exist", self.vault_path.display()));
        }

        let direct_file = self.vault_path.join(format!("{}.md", params.node_id));
        if direct_file.is_file() {
            match std::fs::read_to_string(&direct_file) {
                Ok(content) => return tool_text(content),
                Err(e) => return tool_err(format!("Failed to read {}: {}", direct_file.display(), e)),
            }
        }

        if let Ok(graph) = VaultGraph::from_dir(&self.vault_path) {
            if let Some(vault_node) = graph.get_node(&params.node_id) {
                let file_path = self.vault_path.join(&vault_node.relative_path);
                match std::fs::read_to_string(&file_path) {
                    Ok(content) => return tool_text(content),
                    Err(e) => return tool_err(format!("Failed to read {}: {}", file_path.display(), e)),
                }
            }
        }

        tool_err(format!("Vault Node '{}' not found", params.node_id))
    }

    // 5. Vault Write Canonical
    #[tool(name = "vault_write_canonical", description = "Write or update a canonical OKF v0.2 note in the vault")]
    pub async fn vault_write_canonical(&self, Parameters(params): Parameters<VaultWriteCanonicalParams>) -> CallToolResult {
        if let Err(e) = std::fs::create_dir_all(&self.vault_path) {
            return tool_err(format!("Failed to create vault directory: {}", e));
        }

        let node_type = match params.node_type.to_ascii_lowercase().as_str() {
            "concept" => NodeType::Concept,
            "decision" => NodeType::Decision,
            "architecture" => NodeType::Architecture,
            "incident" => NodeType::Incident,
            "evidence" => NodeType::Evidence,
            "resolution" => NodeType::Resolution,
            "runbook" => NodeType::Runbook,
            "guide" => NodeType::Guide,
            _ => NodeType::Custom(params.node_type.clone()),
        };

        let mut frontmatter = Frontmatter::default();
        frontmatter.id = params.node_id.clone();
        frontmatter.title = params.title;
        frontmatter.node_type = node_type;
        frontmatter.status = NodeStatus::Active;
        frontmatter.tags = params.tags.unwrap_or_default();

        let doc_str = match serialize_document(&frontmatter, &params.content) {
            Ok(s) => s,
            Err(e) => return tool_err(format!("Failed to serialize document: {}", e)),
        };

        let target_path = self.vault_path.join(format!("{}.md", params.node_id));
        match std::fs::write(&target_path, doc_str) {
            Ok(_) => tool_text(format!("Wrote vault node '{}' to {}", params.node_id, target_path.display())),
            Err(e) => tool_err(format!("Failed to write vault node: {}", e)),
        }
    }

    // 6. Graph Query
    #[tool(name = "graph_query", description = "Search code symbols in the code graph using full-text search")]
    pub async fn graph_query(&self, Parameters(params): Parameters<GraphQueryParams>) -> CallToolResult {
        let db_path = self.graph_db_path();
        if !db_path.exists() {
            return tool_err(format!("Code graph database not found at {}", db_path.display()));
        }
        let store = match GraphStore::open(&db_path) {
            Ok(s) => s,
            Err(e) => return tool_err(format!("Failed to open code graph: {}", e)),
        };

        match store.fts_search(&params.query, params.repo_name.as_deref(), params.limit.unwrap_or(20)) {
            Ok(hits) => tool_ok(&hits),
            Err(e) => tool_err(format!("Graph search error: {}", e)),
        }
    }

    // 7. Graph Get Node
    #[tool(name = "graph_get_node", description = "Retrieve code graph node details by symbol ID")]
    pub async fn graph_get_node(&self, Parameters(params): Parameters<GraphGetNodeParams>) -> CallToolResult {
        let db_path = self.graph_db_path();
        if !db_path.exists() {
            return tool_err(format!("Code graph database not found at {}", db_path.display()));
        }
        let store = match GraphStore::open(&db_path) {
            Ok(s) => s,
            Err(e) => return tool_err(format!("Failed to open code graph: {}", e)),
        };

        match store.get_symbol_by_id(&params.node_id) {
            Ok(Some(sym)) => tool_ok(&sym),
            Ok(None) => tool_err(format!("Symbol '{}' not found", params.node_id)),
            Err(e) => tool_err(format!("Failed to query symbol: {}", e)),
        }
    }

    // 8. Graph Get Neighbors
    #[tool(name = "graph_get_neighbors", description = "Retrieve incoming and outgoing edges for a code node")]
    pub async fn graph_get_neighbors(&self, Parameters(params): Parameters<GraphGetNeighborsParams>) -> CallToolResult {
        let db_path = self.graph_db_path();
        if !db_path.exists() {
            return tool_err(format!("Code graph database not found at {}", db_path.display()));
        }
        let store = match GraphStore::open(&db_path) {
            Ok(s) => s,
            Err(e) => return tool_err(format!("Failed to open code graph: {}", e)),
        };

        let dir = params.direction.as_deref().unwrap_or("both");
        let mut outgoing = Vec::new();
        let mut incoming = Vec::new();

        if dir == "outgoing" || dir == "both" {
            outgoing = store.get_outgoing_edges(&params.node_id).unwrap_or_default();
        }
        if dir == "incoming" || dir == "both" {
            incoming = store.get_incoming_edges(&params.node_id).unwrap_or_default();
        }

        let res = serde_json::json!({
            "symbol_id": params.node_id,
            "outgoing": outgoing,
            "incoming": incoming,
        });
        tool_ok(&res)
    }

    // 9. Graph Get Community
    #[tool(name = "graph_get_community", description = "Retrieve Louvain community clusters in the code graph")]
    pub async fn graph_get_community(&self, Parameters(params): Parameters<GraphGetCommunityParams>) -> CallToolResult {
        let db_path = self.graph_db_path();
        if !db_path.exists() {
            return tool_err(format!("Code graph database not found at {}", db_path.display()));
        }
        let store = match GraphStore::open(&db_path) {
            Ok(s) => s,
            Err(e) => return tool_err(format!("Failed to open code graph: {}", e)),
        };

        let comms = store.get_communities("").unwrap_or_default();
        if let Some(target_id) = params.community_id {
            let id_str = target_id.to_string();
            let filtered: Vec<_> = comms.into_iter().filter(|c| c.community_id == id_str).collect();
            tool_ok(&filtered)
        } else {
            tool_ok(&comms)
        }
    }

    // 10. Graph God Nodes
    #[tool(name = "graph_god_nodes", description = "Identify architectural hub and god nodes with high degree centrality")]
    pub async fn graph_god_nodes(&self, Parameters(params): Parameters<GraphGodNodesParams>) -> CallToolResult {
        let db_path = self.graph_db_path();
        if !db_path.exists() {
            return tool_err(format!("Code graph database not found at {}", db_path.display()));
        }
        let store = match GraphStore::open(&db_path) {
            Ok(s) => s,
            Err(e) => return tool_err(format!("Failed to open code graph: {}", e)),
        };

        let repo = params.repo_name.as_deref().unwrap_or("");
        match akashic_engine::analytics::god_nodes(&store, repo, params.top_n.unwrap_or(10)) {
            Ok(nodes) => {
                let mapped: Vec<serde_json::Value> = nodes
                    .into_iter()
                    .map(|n| {
                        serde_json::json!({
                            "symbol": n.symbol,
                            "in_degree": n.in_degree,
                            "out_degree": n.out_degree,
                            "total_degree": n.total_degree,
                        })
                    })
                    .collect();
                tool_ok(&mapped)
            }
            Err(e) => tool_err(format!("Failed to compute god nodes: {}", e)),
        }
    }

    // 11. Graph Stats
    #[tool(name = "graph_stats", description = "Get summary statistics for the code graph")]
    pub async fn graph_stats(&self, Parameters(params): Parameters<GraphStatsParams>) -> CallToolResult {
        let db_path = self.graph_db_path();
        if !db_path.exists() {
            return tool_err(format!("Code graph database not found at {}", db_path.display()));
        }
        let store = match GraphStore::open(&db_path) {
            Ok(s) => s,
            Err(e) => return tool_err(format!("Failed to open code graph: {}", e)),
        };

        let conn = store.connection();
        let sym_count: i64 = conn
            .query_row("SELECT count(*) FROM code_symbols", [], |r| r.get(0))
            .unwrap_or(0);
        let edge_count: i64 = conn
            .query_row("SELECT count(*) FROM code_edges", [], |r| r.get(0))
            .unwrap_or(0);
        let comm_count: i64 = conn
            .query_row("SELECT count(*) FROM communities", [], |r| r.get(0))
            .unwrap_or(0);

        let stats = serde_json::json!({
            "repo_name": params.repo_name,
            "symbols_count": sym_count,
            "edges_count": edge_count,
            "communities_count": comm_count,
        });
        tool_ok(&stats)
    }

    // 12. Graph Shortest Path
    #[tool(name = "graph_shortest_path", description = "Find shortest dependency path between two code nodes")]
    pub async fn graph_shortest_path(&self, Parameters(params): Parameters<GraphShortestPathParams>) -> CallToolResult {
        let db_path = self.graph_db_path();
        if !db_path.exists() {
            return tool_err(format!("Code graph database not found at {}", db_path.display()));
        }
        let store = match GraphStore::open(&db_path) {
            Ok(s) => s,
            Err(e) => return tool_err(format!("Failed to open code graph: {}", e)),
        };

        match akashic_engine::analytics::shortest_path(&store, "", &params.source, &params.target, 10) {
            Ok(Some(path)) => {
                let mapped: Vec<serde_json::Value> = path
                    .into_iter()
                    .map(|s| {
                        serde_json::json!({
                            "from_id": s.from_id,
                            "to_id": s.to_id,
                            "edge_type": s.edge_type,
                            "confidence_tier": format!("{:?}", s.confidence_tier),
                            "confidence_score": s.confidence_score,
                        })
                    })
                    .collect();
                tool_ok(&mapped)
            }
            Ok(None) => tool_text("No path found"),
            Err(e) => tool_err(format!("Failed to calculate path: {}", e)),
        }
    }

    // 13. Graph Blast Radius
    #[tool(name = "graph_blast_radius", description = "Compute transitive blast radius and impact for target symbol")]
    pub async fn graph_blast_radius(&self, Parameters(params): Parameters<GraphBlastRadiusParams>) -> CallToolResult {
        let db_path = self.graph_db_path();
        if !db_path.exists() {
            return tool_err(format!("Code graph database not found at {}", db_path.display()));
        }
        let store = match GraphStore::open(&db_path) {
            Ok(s) => s,
            Err(e) => return tool_err(format!("Failed to open code graph: {}", e)),
        };

        match akashic_engine::analytics::blast_radius(
            &store,
            "",
            &[params.target],
            params.max_depth.unwrap_or(3),
        ) {
            Ok(report) => {
                let report_val = serde_json::json!({
                    "changed_symbols": report.changed_symbols,
                    "upstream_callers": report.upstream_callers,
                    "downstream_dependencies": report.downstream_dependencies,
                    "impacted_communities": report.impacted_communities,
                    "max_depth_reached": report.max_depth_reached,
                });
                tool_ok(&report_val)
            }
            Err(e) => tool_err(format!("Failed to compute blast radius: {}", e)),
        }
    }

    // 14. RCA Status
    #[tool(name = "rca_status", description = "Check status of an ongoing or completed RCA investigation")]
    pub async fn rca_status(&self, Parameters(params): Parameters<RcaStatusParams>) -> CallToolResult {
        let db_path = self.rca_db_path();
        if !db_path.exists() {
            return tool_err(format!("RCA state database not found at {}", db_path.display()));
        }
        let store = match StateStore::open(&db_path) {
            Ok(s) => s,
            Err(e) => return tool_err(format!("Failed to open RCA state store: {}", e)),
        };

        match store.get_workflow_run(&params.investigation_id) {
            Ok(Some(run)) => {
                let checkpoints: Vec<String> = store
                    .get_checkpoints(&params.investigation_id)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|rec| rec.stage_name)
                    .collect();
                let curation_report: Option<serde_json::Value> = run
                    .curation_report_json
                    .as_deref()
                    .and_then(|s| serde_json::from_str(s).ok())
                    .or_else(|| {
                        store
                            .get_curation_report(&params.investigation_id)
                            .ok()
                            .flatten()
                            .and_then(|s| serde_json::from_str(&s).ok())
                    });
                let context_packet_val: Option<serde_json::Value> = store
                    .get_context_packet(&params.investigation_id)
                    .ok()
                    .flatten()
                    .and_then(|p| serde_json::from_str(&p).ok());
                let candidates = store
                    .list_candidate_findings(&params.investigation_id)
                    .unwrap_or_default();
                let res = serde_json::json!({
                    "investigation_id": run.run_id,
                    "jira_key": run.jira_key,
                    "status": run.status,
                    "current_stage": run.current_stage,
                    "crash_signature": run.crash_signature,
                    "created_at": run.created_at,
                    "updated_at": run.updated_at,
                    "checkpoints": checkpoints,
                    "curation_report": curation_report,
                    "workflow_run": run,
                    "context_packet": context_packet_val,
                    "candidate_findings": candidates,
                });
                tool_ok(&res)
            }
            Ok(None) => tool_err(format!("Investigation '{}' not found", params.investigation_id)),
            Err(e) => tool_err(format!("Failed to query investigation: {}", e)),
        }
    }

    // 15. RCA Trigger
    #[tool(name = "rca_trigger", description = "Trigger end-to-end RCA conductor investigation pipeline")]
    pub async fn rca_trigger(&self, Parameters(params): Parameters<RcaTriggerParams>) -> CallToolResult {
        let db_path = self.rca_db_path();
        if let Some(parent) = db_path.parent() {
            if !parent.as_os_str().is_empty() {
                let _ = std::fs::create_dir_all(parent);
            }
        }
        let state_store = match StateStore::open(&db_path) {
            Ok(s) => s,
            Err(_) => match StateStore::open_in_memory() {
                Ok(s) => s,
                Err(e) => return tool_err(format!("Failed to initialize state store: {}", e)),
            },
        };

        let graph_path = self.graph_db_path();
        let graph_store = if graph_path.exists() {
            Some(graph_path)
        } else {
            None
        };

        let pipeline = ConductorPipeline::new(state_store.clone(), MockStage4Executor::new());
        let input = PipelineInput {
            run_id: params.run_id,
            legacy_session_id: None,
            jira_key: params.jira_key.unwrap_or_else(|| "MCP-AUTOTRIGGER".to_string()),
            app_id: params.app_id,
            device: "mcp-device".to_string(),
            android_version: "14".to_string(),
            raw_logs: params.raw_log,
            vault_path: self.vault_path.clone(),
            graph_store,
            prior_resolution_id: params.prior_resolution_id,
            policy_path: None,
        };

        let two_phase = params.two_phase.unwrap_or(true);
        if two_phase {
            match pipeline.prepare_investigation(input) {
                Ok((ctx, packet_opt)) => {
                    let status = if ctx.deduplicated {
                        "DEDUPLICATED"
                    } else {
                        "AWAITING_REASONING"
                    };
                    let res = serde_json::json!({
                        "run_id": ctx.run_id,
                        "incident_id": ctx.incident_id,
                        "status": status,
                        "deduplicated": ctx.deduplicated,
                        "crash_signature": ctx.crash_signature,
                        "context_packet": packet_opt,
                    });
                    tool_ok(&res)
                }
                Err(e) => tool_err(format!("RCA prepare investigation failed: {}", e)),
            }
        } else {
            match pipeline.run(input).await {
                Ok(ctx) => {
                    let packet_val: Option<serde_json::Value> = state_store
                        .get_context_packet(&ctx.run_id)
                        .ok()
                        .flatten()
                        .and_then(|p| serde_json::from_str(&p).ok());
                    let res = serde_json::json!({
                        "run_id": ctx.run_id,
                        "incident_id": ctx.incident_id,
                        "status": if ctx.deduplicated { "DEDUPLICATED" } else { "COMPLETED" },
                        "deduplicated": ctx.deduplicated,
                        "crash_signature": ctx.crash_signature,
                        "incident_node_id": ctx.incident_node_id,
                        "evidence_node_id": ctx.evidence_node_id,
                        "resolution_node_id": ctx.resolution_node_id,
                        "context_packet": packet_val,
                    });
                    tool_ok(&res)
                }
                Err(e) => tool_err(format!("RCA pipeline execution failed: {}", e)),
            }
        }
    }

    // 16. RCA Symbolicate
    #[tool(name = "rca_symbolicate", description = "Parse and symbolicate a stack trace against code graph or mappings")]
    pub async fn rca_symbolicate(&self, Parameters(params): Parameters<RcaSymbolicateParams>) -> CallToolResult {
        let graph_path = self.graph_db_path();
        let resolver: Box<dyn SymbolResolver> = if graph_path.exists() {
            match GraphStore::open(&graph_path) {
                Ok(store) => Box::new(
                    EngineSymbolResolver::new(store)
                        .with_fallback(InMemorySymbolResolver::with_motorola_defaults()),
                ),
                Err(_) => Box::new(InMemorySymbolResolver::with_motorola_defaults()),
            }
        } else {
            Box::new(InMemorySymbolResolver::with_motorola_defaults())
        };

        match symbolicate_crash(&params.app_id, &params.raw_log, resolver.as_ref()) {
            Ok(res) => tool_ok(&res),
            Err(e) => tool_err(format!("Symbolication failed: {}", e)),
        }
    }

    // 17. RCA Crystallize
    #[tool(name = "rca_crystallize", description = "Crystallize RCA investigation findings into canonical vault notes")]
    pub async fn rca_crystallize(&self, Parameters(params): Parameters<RcaCrystallizeParams>) -> CallToolResult {
        if let Err(e) = std::fs::create_dir_all(&self.vault_path) {
            return tool_err(format!("Failed to create vault directory: {}", e));
        }

        let db_path = self.rca_db_path();
        if !db_path.exists() {
            return tool_err(format!(
                "Workflow run '{}' not found in RCA state store; run rca_trigger first",
                params.run_id
            ));
        }

        let store = match StateStore::open(&db_path) {
            Ok(s) => s,
            Err(e) => return tool_err(format!("Failed to open RCA state store: {}", e)),
        };

        match store.get_workflow_run(&params.run_id) {
            Ok(Some(_)) => {}
            Ok(None) => {
                return tool_err(format!(
                    "Workflow run '{}' not found in RCA state store; run rca_trigger first",
                    params.run_id
                ));
            }
            Err(e) => return tool_err(format!("Failed to query workflow run: {}", e)),
        }

        let pipeline = ConductorPipeline::new(store.clone(), MockStage4Executor::new());

        // If winning_candidate_id is set, adjudicate
        if let Some(ref winning_id) = params.winning_candidate_id {
            match pipeline.curate_investigation(&params.run_id, winning_id, &self.vault_path) {
                Ok((_ctx, report)) => {
                    let _ = store.save_curation_report(
                        &params.run_id,
                        &serde_json::to_string(&report).unwrap_or_default(),
                    );
                    return tool_ok(&report);
                }
                Err(e) => return tool_err(format!("Curation adjudication failed: {}", e)),
            }
        }

        let finding_result: Result<CandidateFinding, String> = if let Some(ref file_path) = params.finding_file {
            std::fs::read_to_string(file_path)
                .map_err(|e| format!("Failed to read finding file: {}", e))
                .and_then(|content| {
                    serde_json::from_str::<CandidateFinding>(&content)
                        .map_err(|e| format!("Failed to parse CandidateFinding JSON: {}", e))
                })
        } else {
            let outcome = match params.outcome.as_deref().unwrap_or("resolved").to_ascii_lowercase().as_str() {
                "inconclusive" => FindingOutcome::Inconclusive,
                _ => FindingOutcome::Resolved,
            };
            let primary = params
                .primary_hypothesis
                .or_else(|| params.summary.clone())
                .unwrap_or_else(|| "Investigation finding".to_string());
            let remediation = params
                .remediation_proposal
                .or_else(|| params.summary.clone())
                .unwrap_or_else(|| "Remediation applied".to_string());
            let falsification = params.falsification_attempts.unwrap_or_default();
            let invalidated = params.invalidated_hypotheses.unwrap_or_default();
            let failed = params.failed_attempts.unwrap_or_default();
            let conf = params.confidence_score.unwrap_or(1.0);
            let rem_type = params.remediation_type.or_else(|| Some("permanent_fix".to_string()));
            let hold = params.hold_for_curation.unwrap_or(false);

            Ok(CandidateFinding {
                candidate_id: params.candidate_id,
                agent_id: params.agent_id,
                run_id: Some(params.run_id.clone()),
                storm_candidate_id: None,
                outcome,
                primary_hypothesis: primary,
                falsification_attempts: falsification,
                invalidated_hypotheses: invalidated,
                failed_attempts: failed,
                confidence_score: conf,
                remediation_proposal: remediation,
                remediation_type: rem_type,
                supersedes_resolution_id: None,
                hold_for_curation: hold,
                affected_symbols: vec![],
            })
        };

        let finding = match finding_result {
            Ok(f) => f,
            Err(e) => return tool_err(e),
        };

        match pipeline.submit_finding(&params.run_id, &finding, &self.vault_path) {
            Ok((_ctx, report)) => {
                let _ = store.save_curation_report(
                    &params.run_id,
                    &serde_json::to_string(&report).unwrap_or_default(),
                );
                if report.verdict == CurationVerdict::Rejected {
                    let issues_str = report
                        .issues
                        .iter()
                        .map(|i| format!("{}: {}", i.invariant, i.message))
                        .collect::<Vec<_>>()
                        .join("; ");
                    return tool_err(format!("Curation gate rejected finding: {}", issues_str));
                }
                tool_ok(&report)
            }
            Err(e) => return tool_err(format!("Submit finding failed: {}", e)),
        }
    }
}

pub async fn run_mcp_server(
    vault_path: Option<PathBuf>,
    workspace_path: Option<PathBuf>,
    config: Option<crate::AkashicConfig>,
) -> anyhow::Result<()> {
    use rmcp::service::ServiceExt;
    let vault_path = vault_path.unwrap_or_else(|| {
        config
            .as_ref()
            .map(|c| PathBuf::from(&c.vault.path))
            .unwrap_or_else(|| PathBuf::from("vault"))
    });
    let workspace_path = workspace_path.unwrap_or_else(|| PathBuf::from("."));
    let server = if let Some(cfg) = config {
        AkashicMcpServer::with_config(vault_path, workspace_path, cfg)
    } else {
        AkashicMcpServer::new(vault_path, workspace_path)
    };
    let running = server.serve(rmcp::transport::io::stdio()).await?;
    running.waiting().await?;
    Ok(())
}
