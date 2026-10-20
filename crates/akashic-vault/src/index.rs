use akashic_embedding::EmbeddingProvider;
use akashic_sanitize::SanitizerProfile;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::frontmatter::ParsedDocument;
use crate::graph::VaultGraph;
use crate::VaultError;

fn register_sqlite_vec() {
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| unsafe {
        rusqlite::ffi::sqlite3_auto_extension(Some(std::mem::transmute(
            sqlite_vec::sqlite3_vec_init as *const (),
        )));
    });
}

/// Search hit returned by keyword or hybrid searches.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SearchHit {
    pub id: String,
    pub title: String,
    pub path: String,
    pub score: f64,
    pub snippet: Option<String>,
}

/// Statistics summarizing reindexing results.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndexStats {
    pub nodes_indexed: usize,
    pub relations_indexed: usize,
    pub embeddings_indexed: usize,
}

/// Persistent SQLite + FTS5 + Vector index for a vault.
pub struct VaultIndex {
    db_path: PathBuf,
    conn: Connection,
}

impl VaultIndex {
    /// Opens or creates the SQLite index at `db_path`, configuring WAL mode and creating schemas.
    pub fn open(db_path: &Path) -> Result<Self, VaultError> {
        register_sqlite_vec();
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(db_path)?;

        // Configure WAL mode and performance pragmas
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA foreign_keys = ON;",
        )?;

        // Create tables
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS vault_nodes (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                node_type TEXT NOT NULL,
                status TEXT NOT NULL,
                valid_from TEXT,
                valid_until TEXT,
                confidence REAL,
                scope_json TEXT NOT NULL,
                tags_json TEXT NOT NULL,
                owners_json TEXT NOT NULL,
                sources_json TEXT NOT NULL,
                crash_signature TEXT,
                remediation_type TEXT,
                abandoned_reason TEXT,
                external_refs_json TEXT,
                custom_json TEXT NOT NULL,
                path TEXT NOT NULL,
                body TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS vault_relations (
                source_id TEXT NOT NULL,
                target_id TEXT NOT NULL,
                relation_type TEXT NOT NULL,
                evidence TEXT,
                scope_json TEXT NOT NULL
            );

            CREATE VIRTUAL TABLE IF NOT EXISTS vault_fts USING fts5(
                id,
                title,
                body,
                custom_json
            );

            CREATE VIRTUAL TABLE IF NOT EXISTS vault_vec USING vec0(
                node_id TEXT PRIMARY KEY,
                embedding float[384] distance_metric=cosine
            );",
        )?;

        Ok(Self {
            db_path: db_path.to_path_buf(),
            conn,
        })
    }

    pub fn ensure_vec_table_dimension(&self, dim: usize) -> Result<(), VaultError> {
        if dim == 0 {
            return Ok(());
        }
        let expected_fragment = format!("float[{dim}]");
        let current_sql: Option<String> = self
            .conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'vault_vec'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        let needs_recreate = match current_sql {
            Some(sql) => !sql.contains(&expected_fragment),
            None => true,
        };
        if needs_recreate {
            self.conn.execute_batch(&format!(
                "DROP TABLE IF EXISTS vault_vec;
                 CREATE VIRTUAL TABLE vault_vec USING vec0(
                     node_id TEXT PRIMARY KEY,
                     embedding float[{dim}] distance_metric=cosine
                 );"
            ))?;
        }
        Ok(())
    }

    /// Opens the index at `<vault_root>/.akashic/vault_index.db`.
    pub fn open_in_vault(vault_root: &Path) -> Result<Self, VaultError> {
        let db_path = vault_root.join(".akashic").join("vault_index.db");
        Self::open(&db_path)
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    /// Full reindex of a `VaultGraph`, applying optional sanitization and vector embeddings.
    pub fn reindex(
        &mut self,
        graph: &VaultGraph,
        sanitizer: Option<&dyn SanitizerProfile>,
        embedding: Option<&dyn EmbeddingProvider>,
    ) -> Result<IndexStats, VaultError> {
        if let Some(emb) = embedding {
            if emb.is_available() {
                self.ensure_vec_table_dimension(emb.dimensions())?;
            }
        }

        let mut stats = IndexStats::default();

        let tx = self.conn.transaction()?;

        // Clear existing tables
        tx.execute("DELETE FROM vault_fts;", [])?;
        tx.execute("DELETE FROM vault_vec;", [])?;
        tx.execute("DELETE FROM vault_relations;", [])?;
        tx.execute("DELETE FROM vault_nodes;", [])?;

        // 1. Insert nodes and FTS entries
        let mut node_docs: Vec<&ParsedDocument> = Vec::new();
        let mut sanitized_bodies: Vec<String> = Vec::new();

        for doc in graph.documents.values() {
            let body_content = if let Some(san) = sanitizer {
                san.sanitize(&doc.body).sanitized_content
            } else {
                doc.body.clone()
            };

            let scope_json = serde_json::to_string(&doc.frontmatter.scope)
                .map_err(|e| VaultError::Serialization(e.to_string()))?;
            let tags_json = serde_json::to_string(&doc.frontmatter.tags)
                .map_err(|e| VaultError::Serialization(e.to_string()))?;
            let owners_json = serde_json::to_string(&doc.frontmatter.owners)
                .map_err(|e| VaultError::Serialization(e.to_string()))?;
            let sources_json = serde_json::to_string(&doc.frontmatter.sources)
                .map_err(|e| VaultError::Serialization(e.to_string()))?;
            let ext_refs_json = match &doc.frontmatter.external_refs {
                Some(val) => Some(
                    serde_json::to_string(val)
                        .map_err(|e| VaultError::Serialization(e.to_string()))?,
                ),
                None => None,
            };
            let custom_json = serde_json::to_string(&doc.frontmatter.custom)
                .map_err(|e| VaultError::Serialization(e.to_string()))?;

            tx.execute(
                "INSERT INTO vault_nodes (
                    id, title, node_type, status, valid_from, valid_until, confidence,
                    scope_json, tags_json, owners_json, sources_json, crash_signature,
                    remediation_type, abandoned_reason, external_refs_json, custom_json, path, body
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18);",
                params![
                    doc.frontmatter.id,
                    doc.frontmatter.title,
                    doc.frontmatter.node_type.as_str(),
                    doc.frontmatter.status.as_str(),
                    doc.frontmatter.valid_from,
                    doc.frontmatter.valid_until,
                    doc.frontmatter.confidence,
                    scope_json,
                    tags_json,
                    owners_json,
                    sources_json,
                    doc.frontmatter.crash_signature,
                    doc.frontmatter.remediation_type.map(|r| r.as_str()),
                    doc.frontmatter.abandoned_reason,
                    ext_refs_json,
                    custom_json,
                    doc.relative_path,
                    body_content,
                ],
            )?;

            tx.execute(
                "INSERT INTO vault_fts (id, title, body, custom_json) VALUES (?1, ?2, ?3, ?4);",
                params![
                    doc.frontmatter.id,
                    doc.frontmatter.title,
                    body_content,
                    custom_json,
                ],
            )?;

            node_docs.push(doc);
            sanitized_bodies.push(body_content);
            stats.nodes_indexed += 1;
        }

        // 2. Insert relations
        for doc in graph.documents.values() {
            for rel in &doc.frontmatter.relations {
                let rel_scope_json = serde_json::to_string(&rel.scope)
                    .map_err(|e| VaultError::Serialization(e.to_string()))?;

                tx.execute(
                    "INSERT INTO vault_relations (source_id, target_id, relation_type, evidence, scope_json)
                     VALUES (?1, ?2, ?3, ?4, ?5);",
                    params![
                        doc.frontmatter.id,
                        rel.target,
                        rel.relation_type.as_str(),
                        rel.evidence,
                        rel_scope_json,
                    ],
                )?;
                stats.relations_indexed += 1;
            }
        }

        // 3. Generate embeddings if provider is available
        if let Some(emb) = embedding {
            if emb.is_available() && !node_docs.is_empty() {
                let texts: Vec<String> = node_docs
                    .iter()
                    .zip(sanitized_bodies.iter())
                    .map(|(d, b)| format!("{}\n\n{}", d.frontmatter.title, b))
                    .collect();

                if let Ok(vectors) = emb.embed_batch(&texts) {
                    let mut stmt = tx.prepare(
                        "INSERT INTO vault_vec (node_id, embedding) VALUES (?1, ?2);",
                    )?;
                    for (doc, vec) in node_docs.iter().zip(vectors.into_iter()) {
                        let bytes = zerocopy::IntoBytes::as_bytes(vec.as_slice());
                        stmt.execute(params![doc.frontmatter.id, bytes])?;
                        stats.embeddings_indexed += 1;
                    }
                }
            }
        }

        tx.commit()?;
        Ok(stats)
    }

    /// Convenience wrapper to index graph without sanitizer or embedding sidecar.
    pub fn index_graph(&mut self, graph: &VaultGraph) -> Result<IndexStats, VaultError> {
        self.reindex(graph, None, None)
    }

    /// Keyword search backed by SQLite FTS5 with BM25 ranking.
    pub fn search_keyword(&self, query: &str, limit: usize) -> Result<Vec<SearchHit>, VaultError> {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return Ok(Vec::new());
        }

        // Tokenize query words for safe FTS5 query
        let fts_query = trimmed
            .split_whitespace()
            .map(|token| format!("\"{}\"", token.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(" ");

        let stmt = self.conn.prepare(
            "SELECT f.id, n.title, n.path, bm25(vault_fts) as score,
                    snippet(vault_fts, 2, '<b>', '</b>', '...', 16) as snip
             FROM vault_fts f
             JOIN vault_nodes n ON f.id = n.id
             WHERE vault_fts MATCH ?1
             ORDER BY score ASC
             LIMIT ?2;",
        );

        match stmt {
            Ok(mut query_stmt) => {
                let rows = query_stmt.query_map(params![fts_query, limit], |row| {
                    let id: String = row.get(0)?;
                    let title: String = row.get(1)?;
                    let path: String = row.get(2)?;
                    let raw_score: f64 = row.get(3)?;
                    let snippet: Option<String> = row.get(4)?;
                    let normalized_score = -raw_score; // Higher is better

                    Ok(SearchHit {
                        id,
                        title,
                        path,
                        score: normalized_score,
                        snippet,
                    })
                });

                match rows {
                    Ok(mapped) => {
                        let mut hits = Vec::new();
                        for r in mapped {
                            if let Ok(hit) = r {
                                hits.push(hit);
                            }
                        }
                        if !hits.is_empty() {
                            return Ok(hits);
                        }
                    }
                    Err(_) => {}
                }
            }
            Err(_) => {}
        }

        // Fallback: LIKE query on vault_nodes
        let like_pattern = format!("%{}%", trimmed);
        let mut fallback_stmt = self.conn.prepare(
            "SELECT id, title, path, substr(body, 1, 120) as snip
             FROM vault_nodes
             WHERE title LIKE ?1 OR body LIKE ?1 OR custom_json LIKE ?1
             LIMIT ?2;",
        )?;

        let hits = fallback_stmt
            .query_map(params![like_pattern, limit], |row| {
                let id: String = row.get(0)?;
                let title: String = row.get(1)?;
                let path: String = row.get(2)?;
                let snip: Option<String> = row.get(3)?;
                Ok(SearchHit {
                    id,
                    title,
                    path,
                    score: 1.0,
                    snippet: snip,
                })
            })?
            .filter_map(Result::ok)
            .collect();

        Ok(hits)
    }

    /// Hybrid search combining FTS5 keyword BM25 with dense embedding vector similarity via RRF.
    pub fn search_hybrid(
        &self,
        query: &str,
        embedding: Option<&dyn EmbeddingProvider>,
        limit: usize,
    ) -> Result<Vec<SearchHit>, VaultError> {
        let emb = match embedding {
            Some(e) if e.is_available() => e,
            _ => return self.search_keyword(query, limit),
        };

        // Get query embedding
        let query_vec = match emb.embed_batch(&[query.to_string()]) {
            Ok(vecs) if !vecs.is_empty() => vecs[0].clone(),
            _ => return self.search_keyword(query, limit),
        };

        // 1. Get keyword rank list
        let keyword_hits = self.search_keyword(query, limit * 2)?;
        let mut keyword_rank: HashMap<String, usize> = HashMap::new();
        for (rank, hit) in keyword_hits.iter().enumerate() {
            keyword_rank.insert(hit.id.clone(), rank);
        }

        // 2. Get vector rank list from stored embeddings via sqlite-vec KNN
        let expected_fragment = format!("float[{}]", query_vec.len());
        let current_sql: Option<String> = self
            .conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'vault_vec'",
                [],
                |row| row.get(0),
            )
            .optional()?;

        let mut vector_scores: Vec<(String, f64)> = Vec::new();
        if let Some(sql) = current_sql {
            if sql.contains(&expected_fragment) && !query_vec.is_empty() {
                let k_knn = (limit * 2).max(10).min(4096) as i64;
                let query_bytes = zerocopy::IntoBytes::as_bytes(query_vec.as_slice());
                let mut stmt = self.conn.prepare(
                    "SELECT node_id, distance FROM vault_vec WHERE embedding MATCH ?1 AND k = ?2",
                )?;
                let rows = stmt.query_map(params![query_bytes, k_knn], |row| {
                    let node_id: String = row.get(0)?;
                    let dist_opt: Option<f32> = row.get(1)?;
                    Ok((node_id, dist_opt))
                })?;

                for r in rows {
                    let (node_id, dist_opt) = r?;
                    let sim = 1.0 - (dist_opt.unwrap_or(1.0) as f64);
                    vector_scores.push((node_id, sim));
                }
            }
        }

        vector_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let mut vector_rank: HashMap<String, usize> = HashMap::new();
        for (rank, (node_id, _)) in vector_scores.iter().enumerate() {
            vector_rank.insert(node_id.clone(), rank);
        }

        // 3. Reciprocal Rank Fusion (RRF)
        let mut all_ids: HashSet<String> = HashSet::new();
        all_ids.extend(keyword_rank.keys().cloned());
        all_ids.extend(vector_scores.iter().take(limit * 2).map(|(id, _)| id.clone()));

        let k = 60.0;
        let mut rrf_scores: Vec<(String, f64)> = Vec::new();

        for id in &all_ids {
            let mut score = 0.0;
            if let Some(&k_rank) = keyword_rank.get(id) {
                score += 1.0 / (k + (k_rank as f64) + 1.0);
            }
            if let Some(&v_rank) = vector_rank.get(id) {
                score += 1.0 / (k + (v_rank as f64) + 1.0);
            }
            rrf_scores.push((id.clone(), score));
        }

        rrf_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // 4. Hydrate top hits from vault_nodes
        let mut results = Vec::new();
        let mut node_stmt = self
            .conn
            .prepare("SELECT title, path, substr(body, 1, 120) FROM vault_nodes WHERE id = ?1;")?;

        for (id, score) in rrf_scores.into_iter().take(limit) {
            let hit = node_stmt.query_row(params![id], |row| {
                let title: String = row.get(0)?;
                let path: String = row.get(1)?;
                let snip: Option<String> = row.get(2)?;
                Ok(SearchHit {
                    id: id.clone(),
                    title,
                    path,
                    score,
                    snippet: snip,
                })
            });

            if let Ok(h) = hit {
                results.push(h);
            }
        }

        Ok(results)
    }

    /// Retrieves custom frontmatter fields for a node.
    pub fn get_node_custom_fields(
        &self,
        node_id: &str,
    ) -> Result<Option<BTreeMap<String, serde_yaml::Value>>, VaultError> {
        let mut stmt = self
            .conn
            .prepare("SELECT custom_json FROM vault_nodes WHERE id = ?1;")?;

        let res = stmt.query_row(params![node_id], |row| {
            let json_str: String = row.get(0)?;
            Ok(json_str)
        });

        match res {
            Ok(json_str) => {
                let map: BTreeMap<String, serde_yaml::Value> = serde_json::from_str(&json_str)
                    .map_err(|e| VaultError::Serialization(e.to_string()))?;
                Ok(Some(map))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(VaultError::Database(e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let dir = std::env::temp_dir()
            .join(format!("akashic-vault-index-test-{}-{}", std::process::id(), id));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir)?;
        Ok(TempDir(dir))
    }

    #[test]
    fn test_vault_vec_virtual_table_created() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("vault_index.db");
        let index = VaultIndex::open(&db_path).unwrap();

        // Check that vault_vec virtual table exists
        let table_sql: String = index
            .conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'vault_vec'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(table_sql.contains("vec0"), "vault_vec must use vec0 virtual table");
        assert!(
            table_sql.contains("float[384]"),
            "vault_vec default dimension must be 384"
        );

        // Verify obsolete vault_embeddings table does not exist
        let old_table: Option<String> = index
            .conn
            .query_row(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'vault_embeddings'",
                [],
                |row| row.get(0),
            )
            .optional()
            .unwrap();
        assert!(old_table.is_none(), "vault_embeddings table must not exist");
    }

    #[test]
    fn test_vault_vec_dimension_change() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("vault_index.db");
        let index = VaultIndex::open(&db_path).unwrap();

        index.ensure_vec_table_dimension(4).unwrap();
        let table_sql: String = index
            .conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'vault_vec'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(table_sql.contains("float[4]"));
    }
}
