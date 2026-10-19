pub use serde_yaml;

pub mod crystallizer;
pub mod frontmatter;
pub mod graph;
pub mod index;
pub mod migrator;
pub mod parser;
pub mod query;

pub use crystallizer::*;
pub use frontmatter::*;
pub use graph::*;
pub use index::*;
pub use migrator::*;
pub use parser::*;
pub use query::*;

use akashic_embedding::EmbeddingProvider;
use akashic_sanitize::SanitizerProfile;
use serde::{Deserialize, Serialize};
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum VaultError {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Serialization(String),
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("Conflict error: {0}")]
    Conflict(String),
    #[error("Migration error: {0}")]
    Migration(String),
    #[error("Lock error: {0}")]
    Lock(String),
}

impl From<serde_yaml::Error> for VaultError {
    fn from(e: serde_yaml::Error) -> Self {
        VaultError::Parse(e.to_string())
    }
}

impl From<serde_json::Error> for VaultError {
    fn from(e: serde_json::Error) -> Self {
        VaultError::Serialization(e.to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultConfig {
    pub path: String,
    pub db_path: String,
    pub okf_version: String,
}

impl Default for VaultConfig {
    fn default() -> Self {
        Self {
            path: "vault".to_string(),
            db_path: ".akashic/vault_index.db".to_string(),
            okf_version: "0.2".to_string(),
        }
    }
}

pub struct VaultManager<S: SanitizerProfile, E: EmbeddingProvider> {
    config: VaultConfig,
    sanitizer: S,
    embedding: E,
}

impl<S: SanitizerProfile, E: EmbeddingProvider> VaultManager<S, E> {
    pub fn new(config: VaultConfig, sanitizer: S, embedding: E) -> Self {
        Self {
            config,
            sanitizer,
            embedding,
        }
    }

    pub fn config(&self) -> &VaultConfig {
        &self.config
    }

    pub fn sanitizer(&self) -> &S {
        &self.sanitizer
    }

    pub fn embedding(&self) -> &E {
        &self.embedding
    }

    pub fn sanitize_entry(&self, content: &str) -> akashic_sanitize::SanitizedOutput {
        self.sanitizer.sanitize(content)
    }

    pub fn is_embedding_available(&self) -> bool {
        self.embedding.is_available()
    }

    pub fn open_index(&self) -> Result<VaultIndex, VaultError> {
        let p = Path::new(&self.config.db_path);
        VaultIndex::open(p)
    }

    pub fn load_graph(&self) -> Result<VaultGraph, VaultError> {
        VaultGraph::from_dir(Path::new(&self.config.path))
    }

    pub fn reindex_vault(&self) -> Result<IndexStats, VaultError> {
        let graph = self.load_graph()?;
        let mut idx = self.open_index()?;
        idx.reindex(&graph, Some(&self.sanitizer), Some(&self.embedding))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use akashic_embedding::{EmbeddingError, EmbeddingHealth, OfflineEmbeddingProvider};
    use akashic_sanitize::CodeProfile;
    use std::collections::BTreeMap;
    use std::fs;
    struct TempDir(std::path::PathBuf);
    impl TempDir {
        fn path(&self) -> &std::path::Path {
            &self.0
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn tempdir() -> Result<TempDir, std::io::Error> {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let id = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir =
            std::env::temp_dir().join(format!("akashic-vault-test-{}-{}", std::process::id(), id));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir)?;
        Ok(TempDir(dir))
    }

    #[test]
    fn test_vault_dependency_wiring() {
        let config = VaultConfig::default();
        let sanitizer = CodeProfile::new();
        let embedding = OfflineEmbeddingProvider::default();
        let manager = VaultManager::new(config, sanitizer, embedding);

        let sanitized = manager.sanitize_entry("api_key = \"secret1234567890abcdef\"");
        assert!(sanitized.redactions_count > 0);
        assert!(!manager.is_embedding_available());
    }

    #[test]
    fn test_custom_fields_roundtrip_and_indexing() {
        let dir = tempdir().unwrap();
        let note_path = dir.path().join("decision-cache.md");

        let mut custom = BTreeMap::new();
        custom.insert(
            "owner_team".to_string(),
            serde_yaml::Value::String("platform-core".to_string()),
        );
        custom.insert("sla_tier".to_string(), serde_yaml::Value::Number(1.into()));
        custom.insert(
            "automated_audit".to_string(),
            serde_yaml::Value::Bool(true),
        );

        let frontmatter = Frontmatter {
            id: "DEC-001".to_string(),
            title: "Distributed Cache Strategy".to_string(),
            node_type: NodeType::Decision,
            status: NodeStatus::Active,
            custom: custom.clone(),
            ..Default::default()
        };

        let body = "# Cache Strategy\n\nDetails about redis tiering.";
        let doc_str = serialize_document(&frontmatter, body).unwrap();

        fs::write(&note_path, &doc_str).unwrap();

        // Parse back and verify custom fields
        let parsed = parse_markdown_file(dir.path(), &note_path).unwrap();
        assert_eq!(parsed.frontmatter.id, "DEC-001");
        assert_eq!(
            parsed.frontmatter.custom.get("owner_team"),
            Some(&serde_yaml::Value::String("platform-core".to_string()))
        );
        assert_eq!(
            parsed.frontmatter.custom.get("sla_tier"),
            Some(&serde_yaml::Value::Number(1.into()))
        );
        assert_eq!(
            parsed.frontmatter.custom.get("automated_audit"),
            Some(&serde_yaml::Value::Bool(true))
        );

        // Build graph and index into SQLite
        let graph = VaultGraph::from_dir(dir.path()).unwrap();
        let db_path = dir.path().join(".akashic").join("vault_index.db");
        let mut index = VaultIndex::open(&db_path).unwrap();
        index.index_graph(&graph).unwrap();

        // Retrieve custom fields from SQLite
        let indexed_custom = index.get_node_custom_fields("DEC-001").unwrap().unwrap();
        assert_eq!(
            indexed_custom.get("owner_team"),
            Some(&serde_yaml::Value::String("platform-core".to_string()))
        );
        assert_eq!(
            indexed_custom.get("sla_tier"),
            Some(&serde_yaml::Value::Number(1.into()))
        );

        // FTS search targeting content indexed in custom_json
        let hits = index.search_keyword("platform-core", 5).unwrap();
        assert!(!hits.is_empty());
        assert_eq!(hits[0].id, "DEC-001");
    }

    #[test]
    fn test_forward_only_and_prohibited_reverse_relations() {
        // Strict parser rejects prohibited reverse relations
        let yaml_bad_top = r#"
id: TEST-01
type: concept
status: active
superseded_by: OLD-01
"#;
        assert!(parse_frontmatter_strict(yaml_bad_top).is_err());

        let yaml_bad_rel = r#"
id: TEST-02
type: incident
status: resolved
relations:
  - type: resolved_by
    target: RES-01
"#;
        assert!(parse_frontmatter_strict(yaml_bad_rel).is_err());

        let yaml_bad_req = r#"
id: TEST-03
type: concept
status: active
relations:
  - type: depended_on_by
    target: SYS-01
"#;
        assert!(parse_frontmatter_strict(yaml_bad_req).is_err());

        // Tolerant parser records prohibited reverse relations in ParsedDocument for linter
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("tolerant-test.md");
        let tolerant_content = r#"---
id: TEST-04
type: decision
status: active
superseded_by: ARCHIVED-01
relations:
  - type: supersedes
    target: LEGACY-01
  - type: resolved_by
    target: RES-99
---
# Tolerant Test
"#;
        let parsed = parse_markdown_str(dir.path(), &file_path, tolerant_content).unwrap();
        assert!(parsed
            .prohibited_relations
            .contains(&"superseded_by".to_string()));
        assert!(parsed
            .prohibited_relations
            .contains(&"resolved_by".to_string()));
        // Custom fields must not be polluted by top-level superseded_by
        assert!(!parsed.frontmatter.custom.contains_key("superseded_by"));
        // Valid forward relation preserved
        assert_eq!(parsed.frontmatter.relations.len(), 1);
        assert_eq!(
            parsed.frontmatter.relations[0].relation_type,
            RelationType::Supersedes
        );
    }

    #[test]
    fn test_query_modes_and_unresolved_conflict_abstention() {
        let dir = tempdir().unwrap();

        // 1. Initial active concept note-a
        let note_a = r#"---
id: note-a
type: concept
status: active
title: Old Concept A
scope:
  env: production
---
# Concept A
"#;
        fs::write(dir.path().join("note-a.md"), note_a).unwrap();

        // 2. Successor note-b superseding note-a
        let note_b = r#"---
id: note-b
type: decision
status: active
title: Successor B
scope:
  env: production
relations:
  - type: supersedes
    target: note-a
---
# Decision B
"#;
        fs::write(dir.path().join("note-b.md"), note_b).unwrap();

        // 3. Competing active successor note-c also superseding note-a in overlapping scope
        let note_c = r#"---
id: note-c
type: decision
status: active
title: Competing Successor C
scope:
  env: production
relations:
  - type: supersedes
    target: note-a
---
# Decision C
"#;
        fs::write(dir.path().join("note-c.md"), note_c).unwrap();

        let graph = VaultGraph::from_dir(dir.path()).unwrap();

        let mut scope = Scope::new();
        scope.insert("env".to_string(), "production".to_string());

        // Current mode: must abstain due to competing successors!
        let packet = query_current(&graph, "note-a", Some(&scope)).unwrap();
        assert!(packet.abstained);
        assert!(packet.unresolved_conflict.is_some());
        assert_eq!(packet.focal_nodes.len(), 2);
        let focal_ids: Vec<String> = packet.focal_nodes.iter().map(|n| n.id.clone()).collect();
        assert!(focal_ids.contains(&"note-b".to_string()));
        assert!(focal_ids.contains(&"note-c".to_string()));

        // Lineage mode
        let lineage = query_lineage(&graph, "note-b", Some(&scope)).unwrap();
        assert!(!lineage.abstained);
        assert!(!lineage.ancestors.is_empty());
        assert_eq!(lineage.ancestors[0].id, "note-a");

        // Impact mode
        let impact = query_impact(&graph, "note-a", Some(&scope)).unwrap();
        assert!(!impact.abstained);

        // Historical mode
        let hist_doc = r#"---
id: hist-01
type: decision
status: active
valid_from: "2026-01-01"
valid_until: "2026-06-01"
scope:
  env: production
---
# Historical Doc
"#;
        fs::write(dir.path().join("hist-01.md"), hist_doc).unwrap();
        let graph2 = VaultGraph::from_dir(dir.path()).unwrap();

        let hist_active =
            query_historical(&graph2, "hist-01", Some(&scope), Some("2026-03-01")).unwrap();
        assert!(hist_active.historical_packet.unwrap().is_active);

        let hist_expired =
            query_historical(&graph2, "hist-01", Some(&scope), Some("2026-07-01")).unwrap();
        assert!(!hist_expired.historical_packet.unwrap().is_active);
    }

    #[derive(Debug, Clone)]
    struct MockProvider {
        dims: usize,
    }

    impl EmbeddingProvider for MockProvider {
        fn dimensions(&self) -> usize {
            self.dims
        }
        fn is_available(&self) -> bool {
            true
        }
        fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbeddingError> {
            Ok(texts
                .iter()
                .map(|t| {
                    let mut vec = vec![0.0f32; self.dims];
                    let hash = t.len() as f32;
                    for (i, v) in vec.iter_mut().enumerate() {
                        *v = (hash + i as f32).sin();
                    }
                    vec
                })
                .collect())
        }
        fn health(&self) -> EmbeddingHealth {
            EmbeddingHealth {
                available: true,
                endpoint: "mock".to_string(),
                model: "mock".to_string(),
                dimensions: self.dims,
                latency_ms: Some(1),
                message: "Mock ready".to_string(),
            }
        }
    }

    #[test]
    fn test_sqlite_wal_fts5_and_hybrid_search() {
        let dir = tempdir().unwrap();
        let note_a = r#"---
id: search-01
title: SQLite WAL Architecture
type: architecture
status: active
---
# WAL Mode
SQLite Write-Ahead Logging delivers concurrent reads with a dedicated writer.
"#;
        let note_b = r#"---
id: search-02
title: Microservices Payment Gateway
type: decision
status: active
---
# Payment Service
Handles stripe charges and credit card processing tokens.
"#;
        fs::write(dir.path().join("search-01.md"), note_a).unwrap();
        fs::write(dir.path().join("search-02.md"), note_b).unwrap();

        let graph = VaultGraph::from_dir(dir.path()).unwrap();
        let db_path = dir.path().join(".akashic").join("vault_index.db");
        let mut index = VaultIndex::open(&db_path).unwrap();

        let mock_emb = MockProvider { dims: 4 };
        let stats = index.reindex(&graph, None, Some(&mock_emb)).unwrap();
        assert_eq!(stats.nodes_indexed, 2);
        assert_eq!(stats.embeddings_indexed, 2);

        // Keyword search
        let kw_hits = index.search_keyword("WAL", 5).unwrap();
        assert!(!kw_hits.is_empty());
        assert_eq!(kw_hits[0].id, "search-01");

        // Hybrid search
        let hybrid_hits = index.search_hybrid("Payment", Some(&mock_emb), 5).unwrap();
        assert!(!hybrid_hits.is_empty());
        assert_eq!(hybrid_hits[0].id, "search-02");
    }

    #[test]
    fn test_migrator_locking_and_crystallizer() {
        let dir = tempdir().unwrap();
        let unnorm = r#"---
id: UNNORM-01
title: Legacy Unnormalized Note
type: Concept
status: Active
---
# Legacy Content
"#;
        fs::write(dir.path().join("unnorm.md"), unnorm).unwrap();

        let plan = generate_plan(dir.path()).unwrap();
        assert!(!plan.changes.is_empty());
        assert_eq!(plan.changes[0].path, "unnorm.md");

        let applied = apply_plan(dir.path(), &plan).unwrap();
        assert_eq!(applied, 1);

        // Verify backup created under .akashic/backups/
        let backups_dir = dir.path().join(".akashic").join("backups");
        assert!(backups_dir.exists());

        // Crystallizer test
        let inc = build_incident_node(IncidentInput {
            id: "INC-20261019-01".to_string(),
            title: "Gateway Timeout Outage".to_string(),
            crash_signature: "SIGSEGV 0x00000000".to_string(),
            status: Some(NodeStatus::Resolved),
            scope: Scope::new(),
            external_refs: None,
            relations: Vec::new(),
            body: "# Outage Summary\nGateway crashed on spike.".to_string(),
        })
        .unwrap();
        assert_eq!(inc.frontmatter.node_type, NodeType::Incident);
        assert_eq!(
            inc.frontmatter.crash_signature.as_deref(),
            Some("SIGSEGV 0x00000000")
        );

        let evd = build_evidence_node(EvidenceInput {
            id: "EVD-20261019-01".to_string(),
            title: "Memory Dump Evidence".to_string(),
            incident_id: Some("INC-20261019-01".to_string()),
            status: Some(NodeStatus::Inconclusive),
            scope: Scope::new(),
            invalidated_hypotheses: vec!["Database deadlocks caused crash".to_string()],
            failed_attempts: vec!["Restarted redis cluster".to_string()],
            relations: Vec::new(),
            body: "# Memory Dump\nCaptured core dump from worker.".to_string(),
        })
        .unwrap();
        assert!(evd.rendered_markdown.contains("## Negative Knowledge"));
        assert!(evd
            .rendered_markdown
            .contains("Database deadlocks caused crash"));
        assert!(evd.rendered_markdown.contains("Restarted redis cluster"));

        let res = build_resolution_node(ResolutionInput {
            id: "RES-20261019-01".to_string(),
            title: "Upgrade connection pool".to_string(),
            incident_id: Some("INC-20261019-01".to_string()),
            remediation_type: RemediationType::PermanentFix,
            status: Some(NodeStatus::Resolved),
            abandoned_reason: None,
            scope: Scope::new(),
            external_refs: None,
            relations: Vec::new(),
            body: "# Fix Applied\nIncreased pool size from 10 to 100.".to_string(),
        })
        .unwrap();
        assert_eq!(
            res.frontmatter.remediation_type,
            Some(RemediationType::PermanentFix)
        );
        assert_eq!(res.frontmatter.relations.len(), 1);
        assert_eq!(
            res.frontmatter.relations[0].relation_type,
            RelationType::Resolves
        );

        let written = write_vault_node(dir.path(), "resolutions", &res).unwrap();
        assert!(written.exists());
    }
}
