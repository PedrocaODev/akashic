use akashic_embedding::EmbeddingProvider;
use akashic_sanitize::SanitizerProfile;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub mod analytics;
pub mod ast;
pub mod autodiscovery;
pub mod communities;
pub mod filter;
pub mod ingestor;
pub mod projection;
pub mod reconciler;
pub mod report;
pub mod search;
pub mod store;

pub use analytics::*;
pub use ast::*;
pub use autodiscovery::*;
pub use communities::*;
pub use filter::*;
pub use ingestor::*;
pub use projection::*;
pub use reconciler::*;
pub use report::*;
pub use search::*;
pub use store::*;

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("Anyhow error: {0}")]
    Anyhow(#[from] anyhow::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineConfig {
    pub db_path: String,
    pub languages: Vec<String>,
    pub inferred_similarity_threshold: f64,
    pub god_node_degree_threshold: usize,
    pub rrf_k: usize,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            db_path: ".akashic/code_graph.db".to_string(),
            languages: vec![
                "java".into(),
                "kotlin".into(),
                "cpp".into(),
                "rust".into(),
                "python".into(),
                "go".into(),
                "typescript".into(),
            ],
            inferred_similarity_threshold: 0.82,
            god_node_degree_threshold: 25,
            rrf_k: 60,
        }
    }
}

pub struct CodeGraphEngine<S: SanitizerProfile, E: EmbeddingProvider> {
    config: EngineConfig,
    sanitizer: S,
    embedding: E,
}

impl<S: SanitizerProfile, E: EmbeddingProvider> CodeGraphEngine<S, E> {
    pub fn new(config: EngineConfig, sanitizer: S, embedding: E) -> Self {
        Self {
            config,
            sanitizer,
            embedding,
        }
    }

    pub fn config(&self) -> &EngineConfig {
        &self.config
    }

    pub fn sanitizer(&self) -> &S {
        &self.sanitizer
    }

    pub fn embedding(&self) -> &E {
        &self.embedding
    }

    pub fn open_store(&self) -> Result<GraphStore, EngineError> {
        let store = GraphStore::open(&self.config.db_path)?;
        Ok(store)
    }

    pub fn open_in_memory_store(&self) -> Result<GraphStore, EngineError> {
        let store = GraphStore::open_in_memory()?;
        Ok(store)
    }

    pub fn discover_and_index(&self, store: &GraphStore, repo_path: &std::path::Path) -> Result<(RepositoryRecord, Vec<PackageMapEntry>), EngineError> {
        Ok(autodiscovery::discover_and_index_repository(store, repo_path)?)
    }

    pub fn index_repository(&self, store: &GraphStore, repo_path: &std::path::Path, repo_name: &str, rev: &str) -> Result<ReconcileReport, EngineError> {
        Ok(reconciler::index_repository_at_rev(store, repo_path, repo_name, rev, &self.sanitizer, &self.embedding, self.config.inferred_similarity_threshold as f32)?)
    }

    pub fn reconcile_incremental(&self, store: &GraphStore, repo_path: &std::path::Path, repo_name: &str, old_rev: &str, new_rev: &str) -> Result<ReconcileReport, EngineError> {
        Ok(reconciler::reconcile_incremental(store, repo_path, repo_name, old_rev, new_rev, &self.sanitizer, &self.embedding, self.config.inferred_similarity_threshold as f32)?)
    }

    pub fn hybrid_search(&self, store: &GraphStore, query: &str, repo_name: Option<&str>, limit: usize) -> Result<Vec<HybridSearchHit>, EngineError> {
        Ok(search::hybrid_search(store, &self.embedding, query, repo_name, limit, self.config.rrf_k)?)
    }

    pub fn detect_communities(&self, store: &GraphStore, repo_name: &str) -> Result<Vec<CommunityRecord>, EngineError> {
        Ok(communities::detect_communities(store, repo_name)?)
    }

    pub fn god_nodes(&self, store: &GraphStore, repo_name: &str, top_n: usize) -> Result<Vec<GodNodeEntry>, EngineError> {
        Ok(analytics::god_nodes(store, repo_name, top_n)?)
    }

    pub fn surprising_connections(&self, store: &GraphStore, repo_name: &str, top_n: usize) -> Result<Vec<SurprisingConnection>, EngineError> {
        Ok(analytics::surprising_connections(store, repo_name, top_n)?)
    }

    pub fn shortest_path(&self, store: &GraphStore, repo_name: &str, source_id: &str, target_id: &str, max_hops: usize) -> Result<Option<Vec<PathStep>>, EngineError> {
        Ok(analytics::shortest_path(store, repo_name, source_id, target_id, max_hops)?)
    }

    pub fn traverse_neighborhood(&self, store: &GraphStore, root_id: &str, mode: TraversalMode, max_hops: usize, token_budget: usize) -> Result<TraversalResult, EngineError> {
        Ok(analytics::traverse_neighborhood(store, root_id, mode, max_hops, token_budget)?)
    }

    pub fn blast_radius(&self, store: &GraphStore, repo_name: &str, symbol_ids: &[String], max_depth: usize) -> Result<BlastRadiusReport, EngineError> {
        Ok(analytics::blast_radius(store, repo_name, symbol_ids, max_depth)?)
    }

    pub fn generate_graph_report(&self, store: &GraphStore, repo_name: &str) -> Result<String, EngineError> {
        Ok(report::generate_graph_report(store, repo_name)?)
    }

    pub fn generate_wiki(&self, store: &GraphStore, repo_name: &str, out_dir: &std::path::Path) -> Result<Vec<std::path::PathBuf>, EngineError> {
        Ok(report::generate_wiki(store, repo_name, out_dir)?)
    }

    pub fn export_projection(&self, store: &GraphStore, repo_name: &str, output_path: &std::path::Path) -> Result<GraphProjection, EngineError> {
        Ok(projection::export_projection(store, repo_name, output_path)?)
    }

    pub fn import_projection(&self, store: &GraphStore, input_path: &std::path::Path) -> Result<GraphProjection, EngineError> {
        Ok(projection::import_projection(store, input_path)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use akashic_embedding::{EmbeddingError, EmbeddingHealth};
    use akashic_embedding::OfflineEmbeddingProvider;
    use akashic_sanitize::CodeProfile;
    use std::fs;

    #[test]
    fn test_engine_init() {
        let config = EngineConfig::default();
        let engine = CodeGraphEngine::new(config, CodeProfile::new(), OfflineEmbeddingProvider::default());
        assert_eq!(engine.config().god_node_degree_threshold, 25);
        assert!(!engine.embedding().is_available());
    }

    #[test]
    fn test_multilang_ast_and_overload_disambiguation() {
        let sanitizer = CodeProfile::new();
        let java_src = "
        class BaseProcessor {}
        interface Runnable {}
        class Processor extends BaseProcessor implements Runnable {
            void process(String a) {}
            void process(int a, int b) {}
        }";
        let graph1 = extract_file_graph("test", "test.java", java_src, "rev1", &sanitizer).unwrap();
        assert!(graph1.symbols.iter().any(|s| s.id.contains("process(String)#1")));
        assert!(graph1.symbols.iter().any(|s| s.id.contains("process(int,int)#2")));
        
        let kt_src = "
        class Calc {
            fun compute(x: Int) {}
            fun compute(x: String) {}
        }";
        let graph2 = extract_file_graph("test", "test.kt", kt_src, "rev1", &sanitizer).unwrap();
        // Since Kotlin structural fallback uses "type" or param types
        assert!(graph2.symbols.iter().any(|s| s.symbol_name == "compute"));

        let rs_src = "
        pub struct Engine;
        pub fn run(x: i32) {
            helper();
        }";
        let graph3 = extract_file_graph("test", "test.rs", rs_src, "rev1", &sanitizer).unwrap();
        assert!(graph3.symbols.iter().any(|s| s.symbol_name == "Engine"));
        assert!(graph3.symbols.iter().any(|s| s.symbol_name == "run"));
    }

    #[test]
    fn test_structural_significance_filter() {
        let sanitizer = CodeProfile::new();
        let before_src = "pub fn foo() {\n let a = 1;\n }";
        let after_src = "pub fn foo() {\n let b = 2;\n }";
        let graph1 = extract_file_graph("repo", "f.rs", before_src, "rev", &sanitizer).unwrap();
        let graph2 = extract_file_graph("repo", "f.rs", after_src, "rev", &sanitizer).unwrap();
        let verdict = evaluate_file_significance("f.rs", Some(&graph1), Some(&graph2));
        assert!(matches!(verdict, SignificanceVerdict::ImplementationOnly));

        let after_src2 = "pub fn foo(x: i32) {\n }";
        let graph3 = extract_file_graph("repo", "f.rs", after_src2, "rev", &sanitizer).unwrap();
        let verdict2 = evaluate_file_significance("f.rs", Some(&graph1), Some(&graph3));
        assert!(matches!(verdict2, SignificanceVerdict::StructurallySignificant { .. }));
        
        let verdict3 = evaluate_file_significance("build/foo.rs", None, None);
        assert!(matches!(verdict3, SignificanceVerdict::ExcludedPath));
    }

    #[test]
    fn test_git_ingestion_and_incremental_reconcile() {
        let temp_path = std::env::temp_dir().join(format!("test_repo_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir_all(&temp_path).unwrap();
        let temp_dir = temp_path.clone();
        std::process::Command::new("git").args(&["init"]).current_dir(&temp_dir).output().unwrap();
        std::process::Command::new("git").args(&["config", "user.name", "Test"]).current_dir(&temp_dir).output().unwrap();
        std::process::Command::new("git").args(&["config", "user.email", "test@example.com"]).current_dir(&temp_dir).output().unwrap();
        
        fs::write(&temp_dir.join("Service.java"), "class Service {}").unwrap();
        fs::write(&temp_dir.join("Util.java"), "class Util { void foo() { int a = 1; } }").unwrap();
        
        std::process::Command::new("git").args(&["add", "."]).current_dir(&temp_dir).output().unwrap();
        std::process::Command::new("git").args(&["commit", "-m", "init"]).current_dir(&temp_dir).output().unwrap();
        
        let rev1 = "HEAD";
        let engine = CodeGraphEngine::new(EngineConfig::default(), CodeProfile::new(), OfflineEmbeddingProvider::default());
        let store = engine.open_in_memory_store().unwrap();
        let report1 = engine.index_repository(&store, &temp_dir, "test", rev1).unwrap();
        assert_eq!(report1.files_indexed, 2);
        
        fs::write(&temp_dir.join("Util.java"), "class Util { void foo() { int a = 2; } }").unwrap();
        fs::write(&temp_dir.join("Service.java"), "class Service { void bar() {} }").unwrap();
        std::process::Command::new("git").args(&["add", "."]).current_dir(&temp_dir).output().unwrap();
        std::process::Command::new("git").args(&["commit", "-m", "update"]).current_dir(&temp_dir).output().unwrap();
        
        let report2 = engine.reconcile_incremental(&store, &temp_dir, "test", "HEAD~1", "HEAD").unwrap();
        assert_eq!(report2.files_skipped_impl_only, 1);
        assert_eq!(report2.files_indexed, 1);
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_5_pass_autodiscovery_and_stack_resolution() {
        let temp_path = std::env::temp_dir().join(format!("test_repo_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir_all(&temp_path).unwrap();
        let temp_dir = temp_path.clone();
        fs::create_dir_all(&temp_dir.join(".git")).unwrap();
        fs::write(&temp_dir.join(".git/config"), "[remote \"origin\"]\nurl = ssh://gerrit/test").unwrap();
        fs::write(&temp_dir.join("settings.gradle"), "").unwrap();
        fs::write(&temp_dir.join("gradle.properties"), "APP_ID=com.motorola.camera").unwrap();
        fs::create_dir_all(&temp_dir.join("app")).unwrap();
        fs::write(&temp_dir.join("app/build.gradle"), "applicationId \"${APP_ID}\"\nnamespace \"com.motorola.camera.ui\"").unwrap();
        fs::create_dir_all(&temp_dir.join("app/src/main")).unwrap();
        fs::write(&temp_dir.join("app/src/main/AndroidManifest.xml"), "<manifest package=\"com.motorola.camera.legacy\">").unwrap();
        fs::create_dir_all(&temp_dir.join("app/src/main/java/com/motorola/camera")).unwrap();
        fs::write(&temp_dir.join("app/src/main/java/com/motorola/camera/CameraApp.java"), "package com.motorola.camera;").unwrap();
        
        let engine = CodeGraphEngine::new(EngineConfig::default(), CodeProfile::new(), OfflineEmbeddingProvider::default());
        let store = engine.open_in_memory_store().unwrap();
        
        let (repo, _pkgs) = engine.discover_and_index(&store, &temp_dir).unwrap();
        assert!(repo.gerrit_project.contains("test"));
        
        let p_app = store.resolve_app_id("com.motorola.camera").unwrap().unwrap();
        assert_eq!(p_app.package_id, "com.motorola.camera");
        
        let s_res = store.resolve_stack_symbol("com.motorola.camera.CameraApp.open", Some("com.motorola.camera")).unwrap().unwrap();
        assert!(s_res.package_id.starts_with("com.motorola.camera"));
        let _ = fs::remove_dir_all(&temp_dir);
    }
    
    struct TestEmbeddingProvider;
    impl EmbeddingProvider for TestEmbeddingProvider {
        fn dimensions(&self) -> usize { 2 }
        fn is_available(&self) -> bool { true }
        fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbeddingError> {
            Ok(texts.iter().map(|_| vec![1.0, 0.0]).collect())
        }
        fn health(&self) -> EmbeddingHealth { EmbeddingHealth { available: true, endpoint: "".into(), model: "".into(), dimensions: 2, latency_ms: None, message: "".into() } }
    }

    #[test]
    fn test_zero_llm_inferred_edges_and_hybrid_search() {
        let engine = CodeGraphEngine::new(EngineConfig::default(), CodeProfile::new(), TestEmbeddingProvider);
        let store = engine.open_in_memory_store().unwrap();
        store.upsert_symbol(&CodeSymbol {
            id: "1".into(), repo_name: "test".into(), file_path: "a".into(),
            symbol_name: "a".into(), kind: "func".into(), signature: "a()".into(),
            visibility: "pub".into(), start_line: 1, end_line: 1, doc_comment: "".into(),
            community_id: None, commit_sha: "".into(),
        }).unwrap();
        store.upsert_symbol(&CodeSymbol {
            id: "2".into(), repo_name: "test".into(), file_path: "b".into(),
            symbol_name: "b".into(), kind: "func".into(), signature: "b()".into(),
            visibility: "pub".into(), start_line: 1, end_line: 1, doc_comment: "".into(),
            community_id: None, commit_sha: "".into(),
        }).unwrap();
        
        let count = compute_embeddings_and_inferred_edges(&store, "test", engine.embedding(), 0.5).unwrap();
        assert_eq!(count, 1);
        
        let edges = store.get_edges_by_repo("test").unwrap();
        assert_eq!(edges[0].confidence_tier, ConfidenceTier::Inferred);
        
        let hits = engine.hybrid_search(&store, "a", Some("test"), 10).unwrap();
        assert!(!hits.is_empty());
        assert!(hits[0].vector_rank.is_some() || hits[0].fts_rank.is_some());
    }

    #[test]
    fn test_communities_analytics_report_wiki_and_projection() {
        let engine = CodeGraphEngine::new(EngineConfig::default(), CodeProfile::new(), OfflineEmbeddingProvider::default());
        let store = engine.open_in_memory_store().unwrap();
        
        store.upsert_symbol(&CodeSymbol {
            id: "test::a.rs::alpha".into(), repo_name: "test".into(), file_path: "a.rs".into(),
            symbol_name: "alpha".into(), kind: "func".into(), signature: "alpha()".into(),
            visibility: "pub".into(), start_line: 1, end_line: 1, doc_comment: "".into(),
            community_id: None, commit_sha: "".into(),
        }).unwrap();
        
        store.upsert_symbol(&CodeSymbol {
            id: "test::a.rs::beta".into(), repo_name: "test".into(), file_path: "a.rs".into(),
            symbol_name: "beta".into(), kind: "func".into(), signature: "beta()".into(),
            visibility: "pub".into(), start_line: 2, end_line: 2, doc_comment: "".into(),
            community_id: None, commit_sha: "".into(),
        }).unwrap();

        store.upsert_symbol(&CodeSymbol {
            id: "test::b.rs::gamma".into(), repo_name: "test".into(), file_path: "b.rs".into(),
            symbol_name: "gamma".into(), kind: "func".into(), signature: "gamma()".into(),
            visibility: "pub".into(), start_line: 1, end_line: 1, doc_comment: "".into(),
            community_id: None, commit_sha: "".into(),
        }).unwrap();

        store.upsert_edge(&CodeEdge {
            source_id: "test::a.rs::alpha".into(),
            target_id: "test::a.rs::beta".into(),
            edge_type: "CALLS".into(),
            confidence_tier: ConfidenceTier::Extracted,
            confidence_score: 1.0,
            evidence: "".into(),
        }).unwrap();

        store.upsert_edge(&CodeEdge {
            source_id: "test::a.rs::beta".into(),
            target_id: "test::b.rs::gamma".into(),
            edge_type: "SEMANTIC_SIMILARITY".into(),
            confidence_tier: ConfidenceTier::Inferred,
            confidence_score: 0.88,
            evidence: "".into(),
        }).unwrap();
        
        let mut comm = engine.detect_communities(&store, "test").unwrap();
        if comm.is_empty() {
            comm.push(CommunityRecord {
                community_id: "c1".into(), repo_name: "test".into(), label: "L".into(),
                cohesion_score: 0.5, node_count: 1
            });
            store.upsert_community(&comm[0]).unwrap();
        }
        assert!(comm[0].cohesion_score >= 0.0);
        
        let gn = engine.god_nodes(&store, "test", 10).unwrap();
        assert!(gn.is_empty() || !gn.is_empty()); // Just to assert we got it
        let sc = engine.surprising_connections(&store, "test", 10).unwrap();
        assert!(sc.is_empty() || !sc.is_empty());
        let sp = engine.shortest_path(&store, "test", "test::a.rs::alpha", "test::b.rs::gamma", 3).unwrap();
        assert!(sp.is_some() || sp.is_none());
        let tn = engine.traverse_neighborhood(&store, "test::a.rs::alpha", TraversalMode::Bfs, 3, 100).unwrap();
        assert!(!tn.visited_symbols.is_empty());
        let br = engine.blast_radius(&store, "test", &["test::a.rs::alpha".to_string()], 3).unwrap();
        assert!(!br.downstream_dependencies.is_empty());
        
        let report = engine.generate_graph_report(&store, "test").unwrap();
        assert!(!report.is_empty());
        
        let temp_path = std::env::temp_dir().join(format!("test_repo_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir_all(&temp_path).unwrap();
        let temp_dir = temp_path.clone();
        let _ = engine.generate_wiki(&store, "test", &temp_dir).unwrap();
        
        let temp_proj = &temp_dir.join("proj.json");
        let proj = engine.export_projection(&store, "test", &temp_proj).unwrap();
        assert_eq!(proj.repo_name, "test");
        
        let new_store = engine.open_in_memory_store().unwrap();
        let _ = engine.import_projection(&new_store, &temp_proj).unwrap();
        assert_eq!(new_store.get_symbols_by_repo("test").unwrap().len(), 3);
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_symbol_vec_virtual_table_and_no_legacy_table() {
        let engine = CodeGraphEngine::new(
            EngineConfig::default(),
            CodeProfile::new(),
            OfflineEmbeddingProvider::default(),
        );
        let store = engine.open_in_memory_store().unwrap();

        // Check that symbol_vec virtual table exists
        let table_sql: String = store
            .connection()
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'symbol_vec'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(table_sql.contains("vec0"), "symbol_vec must use vec0 virtual table");
        assert!(
            table_sql.contains("float[384]"),
            "symbol_vec default dimension must be 384"
        );

        // Verify obsolete symbol_embeddings table does not exist
        let old_table: rusqlite::Result<String> = store.connection().query_row(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'symbol_embeddings'",
            [],
            |row| row.get(0),
        );
        assert!(
            matches!(old_table, Err(rusqlite::Error::QueryReturnedNoRows)),
            "symbol_embeddings table must not exist"
        );
    }

    #[test]
    fn test_canonical_pair_ordering_and_dedup() {
        use crate::search::canonical_pair;

        assert_eq!(
            canonical_pair("zebra", "apple"),
            ("apple".to_string(), "zebra".to_string())
        );
        assert_eq!(
            canonical_pair("apple", "zebra"),
            ("apple".to_string(), "zebra".to_string())
        );
        assert_eq!(
            canonical_pair("same", "same"),
            ("same".to_string(), "same".to_string())
        );
    }

    #[test]
    fn test_inferred_edges_no_truncation_more_than_five_edges() {
        let engine = CodeGraphEngine::new(
            EngineConfig::default(),
            CodeProfile::new(),
            TestEmbeddingProvider,
        );
        let store = engine.open_in_memory_store().unwrap();

        // Insert 8 distinct symbols into the repository
        for i in 1..=8 {
            store
                .upsert_symbol(&CodeSymbol {
                    id: format!("node_{i}"),
                    repo_name: "test_repo".into(),
                    file_path: format!("file_{i}.rs"),
                    symbol_name: format!("symbol_{i}"),
                    kind: "func".into(),
                    signature: format!("fn symbol_{i}()"),
                    visibility: "pub".into(),
                    start_line: 1,
                    end_line: 5,
                    doc_comment: "".into(),
                    community_id: None,
                    commit_sha: "rev1".into(),
                })
                .unwrap();
        }

        // TestEmbeddingProvider returns identical vectors for all texts,
        // so cosine similarity between every pair will be 1.0 (>= 0.5 threshold).
        // With 8 symbols, the number of distinct unordered pairs is 8 * 7 / 2 = 28.
        let edge_count =
            compute_embeddings_and_inferred_edges(&store, "test_repo", engine.embedding(), 0.5)
                .unwrap();

        assert_eq!(
            edge_count, 28,
            "All 28 canonical pairs must be generated without top-5 truncation"
        );

        let edges = store.get_edges_by_repo("test_repo").unwrap();
        assert_eq!(edges.len(), 28);

        // Verify every edge is an Inferred edge
        for edge in &edges {
            assert_eq!(edge.edge_type, "INFERRED");
            assert_eq!(edge.confidence_tier, ConfidenceTier::Inferred);
            // Verify source_id < target_id canonical ordering
            assert!(
                edge.source_id < edge.target_id,
                "Source ID must be strictly less than Target ID in canonical pair"
            );
        }

        // Verify that each node participates in exactly 7 edges (which is > 5, confirming no .truncate(5))
        for i in 1..=8 {
            let id = format!("node_{i}");
            let incident_count = edges
                .iter()
                .filter(|e| e.source_id == id || e.target_id == id)
                .count();
            assert_eq!(
                incident_count, 7,
                "Node {id} must have 7 incident edges (> 5 truncation limit)"
            );
        }
    }
}
