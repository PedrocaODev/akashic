use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConfidenceTier {
    Extracted,
    Inferred,
    Ambiguous,
}

impl ConfidenceTier {
    pub fn as_str(&self) -> &'static str {
        match self {
            ConfidenceTier::Extracted => "EXTRACTED",
            ConfidenceTier::Inferred => "INFERRED",
            ConfidenceTier::Ambiguous => "AMBIGUOUS",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Self {
        match s {
            "EXTRACTED" => ConfidenceTier::Extracted,
            "INFERRED" => ConfidenceTier::Inferred,
            "AMBIGUOUS" => ConfidenceTier::Ambiguous,
            _ => ConfidenceTier::Ambiguous,
        }
    }

    pub fn default_score(&self) -> f64 {
        match self {
            ConfidenceTier::Extracted => 1.0,
            ConfidenceTier::Inferred => 0.82,
            ConfidenceTier::Ambiguous => 0.5,
        }
    }
}

pub type EdgeProvenance = ConfidenceTier;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CodeSymbol {
    pub id: String,
    pub repo_name: String,
    pub file_path: String,
    pub symbol_name: String,
    pub kind: String,
    pub signature: String,
    pub visibility: String,
    pub start_line: usize,
    pub end_line: usize,
    pub doc_comment: String,
    pub community_id: Option<String>,
    pub commit_sha: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeEdge {
    pub source_id: String,
    pub target_id: String,
    pub edge_type: String,
    pub confidence_tier: ConfidenceTier,
    pub confidence_score: f64,
    pub evidence: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryRecord {
    pub repo_name: String,
    pub worktree_path: String,
    pub gradle_root_path: String,
    pub gerrit_project: String,
    pub primary_app_id: String,
    pub vault_partition: String,
    pub last_indexed_sha: String,
    pub updated_at: i64,
}

pub type AndroidRepository = RepositoryRecord;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageMapEntry {
    pub package_id: String,
    pub repo_name: String,
    pub source_type: String,
    pub module_gradle_path: String,
    pub role_priority: i32,
    pub flavor_name: String,
    pub is_primary_app: bool,
}

pub type AndroidPackageEntry = PackageMapEntry;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommunityRecord {
    pub community_id: String,
    pub repo_name: String,
    pub label: String,
    pub cohesion_score: f64,
    pub node_count: usize,
}

pub struct GraphStore {
    conn: Connection,
}

impl GraphStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        Self::setup(&conn)?;
        Ok(Self { conn })
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        Self::setup(&conn)?;
        Ok(Self { conn })
    }

    fn setup(conn: &Connection) -> Result<()> {
        conn.execute_batch(
            r#"
            PRAGMA journal_mode=WAL;
            PRAGMA synchronous=NORMAL;
            PRAGMA foreign_keys=ON;

            CREATE TABLE IF NOT EXISTS repositories (
                repo_name TEXT PRIMARY KEY,
                worktree_path TEXT NOT NULL,
                gradle_root_path TEXT NOT NULL DEFAULT '',
                gerrit_project TEXT NOT NULL DEFAULT '',
                primary_app_id TEXT NOT NULL DEFAULT '',
                vault_partition TEXT NOT NULL DEFAULT '',
                last_indexed_sha TEXT NOT NULL DEFAULT '',
                updated_at INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS package_map (
                package_id TEXT NOT NULL,
                repo_name TEXT NOT NULL,
                source_type TEXT NOT NULL,
                module_gradle_path TEXT NOT NULL DEFAULT '',
                role_priority INTEGER NOT NULL DEFAULT 5,
                flavor_name TEXT NOT NULL DEFAULT '',
                is_primary_app INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (package_id, repo_name, module_gradle_path, source_type, flavor_name)
            );

            CREATE TABLE IF NOT EXISTS code_symbols (
                id TEXT PRIMARY KEY,
                repo_name TEXT NOT NULL,
                file_path TEXT NOT NULL,
                symbol_name TEXT NOT NULL,
                kind TEXT NOT NULL,
                signature TEXT NOT NULL,
                visibility TEXT NOT NULL DEFAULT 'public',
                start_line INTEGER NOT NULL,
                end_line INTEGER NOT NULL,
                doc_comment TEXT NOT NULL DEFAULT '',
                community_id TEXT,
                commit_sha TEXT NOT NULL DEFAULT ''
            );

            CREATE TABLE IF NOT EXISTS code_edges (
                source_id TEXT NOT NULL,
                target_id TEXT NOT NULL,
                edge_type TEXT NOT NULL,
                confidence_tier TEXT NOT NULL,
                confidence_score REAL NOT NULL,
                evidence TEXT NOT NULL DEFAULT '',
                PRIMARY KEY (source_id, target_id, edge_type)
            );

            CREATE VIRTUAL TABLE IF NOT EXISTS symbol_fts USING fts5(
                id UNINDEXED,
                repo_name UNINDEXED,
                symbol_name,
                signature,
                doc_comment,
                file_path
            );

            CREATE TABLE IF NOT EXISTS symbol_embeddings (
                symbol_id TEXT PRIMARY KEY,
                repo_name TEXT NOT NULL,
                vector_json TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS communities (
                community_id TEXT PRIMARY KEY,
                repo_name TEXT NOT NULL,
                label TEXT NOT NULL,
                cohesion_score REAL NOT NULL,
                node_count INTEGER NOT NULL
            );
            "#
        )?;
        Ok(())
    }

    pub fn upsert_repository(&self, repo: &RepositoryRecord) -> Result<()> {
        self.conn.execute(
            "INSERT INTO repositories (repo_name, worktree_path, gradle_root_path, gerrit_project, primary_app_id, vault_partition, last_indexed_sha, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(repo_name) DO UPDATE SET
                worktree_path=excluded.worktree_path,
                gradle_root_path=excluded.gradle_root_path,
                gerrit_project=excluded.gerrit_project,
                primary_app_id=excluded.primary_app_id,
                vault_partition=excluded.vault_partition,
                last_indexed_sha=excluded.last_indexed_sha,
                updated_at=excluded.updated_at",
            params![repo.repo_name, repo.worktree_path, repo.gradle_root_path, repo.gerrit_project, repo.primary_app_id, repo.vault_partition, repo.last_indexed_sha, repo.updated_at],
        )?;
        Ok(())
    }
    
    pub fn get_repository(&self, repo_name: &str) -> Result<Option<RepositoryRecord>> {
        let mut stmt = self.conn.prepare("SELECT repo_name, worktree_path, gradle_root_path, gerrit_project, primary_app_id, vault_partition, last_indexed_sha, updated_at FROM repositories WHERE repo_name = ?1")?;
        let repo = stmt.query_row(params![repo_name], |row| {
            Ok(RepositoryRecord {
                repo_name: row.get(0)?,
                worktree_path: row.get(1)?,
                gradle_root_path: row.get(2)?,
                gerrit_project: row.get(3)?,
                primary_app_id: row.get(4)?,
                vault_partition: row.get(5)?,
                last_indexed_sha: row.get(6)?,
                updated_at: row.get(7)?,
            })
        }).optional()?;
        Ok(repo)
    }

    pub fn upsert_package_map(&self, entry: &PackageMapEntry) -> Result<()> {
        self.conn.execute(
            "INSERT INTO package_map (package_id, repo_name, source_type, module_gradle_path, role_priority, flavor_name, is_primary_app)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(package_id, repo_name, module_gradle_path, source_type, flavor_name) DO UPDATE SET
                role_priority=excluded.role_priority,
                is_primary_app=excluded.is_primary_app",
            params![entry.package_id, entry.repo_name, entry.source_type, entry.module_gradle_path, entry.role_priority, entry.flavor_name, entry.is_primary_app],
        )?;
        Ok(())
    }

    pub fn get_package_map(&self, repo_name: &str) -> Result<Vec<PackageMapEntry>> {
        let mut stmt = self.conn.prepare("SELECT package_id, repo_name, source_type, module_gradle_path, role_priority, flavor_name, is_primary_app FROM package_map WHERE repo_name = ?1")?;
        let res = stmt.query_map(params![repo_name], |row| {
            Ok(PackageMapEntry {
                package_id: row.get(0)?,
                repo_name: row.get(1)?,
                source_type: row.get(2)?,
                module_gradle_path: row.get(3)?,
                role_priority: row.get(4)?,
                flavor_name: row.get(5)?,
                is_primary_app: row.get(6)?,
            })
        })?;
        let mut out = Vec::new();
        for r in res { out.push(r?); }
        Ok(out)
    }

    pub fn clear_package_map_by_repo(&self, repo_name: &str) -> Result<()> {
        self.conn.execute("DELETE FROM package_map WHERE repo_name = ?1", params![repo_name])?;
        Ok(())
    }

    pub fn resolve_app_id(&self, app_id: &str) -> Result<Option<PackageMapEntry>> {
        let mut stmt = self.conn.prepare("SELECT package_id, repo_name, source_type, module_gradle_path, role_priority, flavor_name, is_primary_app FROM package_map WHERE package_id = ?1 ORDER BY role_priority ASC, is_primary_app DESC LIMIT 1")?;
        let res = stmt.query_row(params![app_id], |row| {
            Ok(PackageMapEntry {
                package_id: row.get(0)?,
                repo_name: row.get(1)?,
                source_type: row.get(2)?,
                module_gradle_path: row.get(3)?,
                role_priority: row.get(4)?,
                flavor_name: row.get(5)?,
                is_primary_app: row.get(6)?,
            })
        }).optional()?;
        Ok(res)
    }

    pub fn resolve_stack_symbol(&self, fqcn: &str, app_id_hint: Option<&str>) -> Result<Option<PackageMapEntry>> {
        let mut stmt = self.conn.prepare("SELECT package_id, repo_name, source_type, module_gradle_path, role_priority, flavor_name, is_primary_app FROM package_map")?;
        let mut entries = Vec::new();
        for row in stmt.query_map([], |row| {
            Ok(PackageMapEntry {
                package_id: row.get(0)?,
                repo_name: row.get(1)?,
                source_type: row.get(2)?,
                module_gradle_path: row.get(3)?,
                role_priority: row.get(4)?,
                flavor_name: row.get(5)?,
                is_primary_app: row.get(6)?,
            })
        })? {
            entries.push(row?);
        }

        let hint_repo = if let Some(app) = app_id_hint {
            self.resolve_app_id(app)?.map(|e| e.repo_name)
        } else { None };

        let mut candidates = Vec::new();
        for e in entries {
            if fqcn == e.package_id || fqcn.starts_with(&format!("{}.", e.package_id)) {
                candidates.push(e);
            }
        }

        candidates.sort_by(|a, b| {
            let a_match = Some(&a.repo_name) == hint_repo.as_ref();
            let b_match = Some(&b.repo_name) == hint_repo.as_ref();
            if a_match != b_match { return b_match.cmp(&a_match); }
            let len_cmp = b.package_id.len().cmp(&a.package_id.len());
            if len_cmp != std::cmp::Ordering::Equal { return len_cmp; }
            let prio_cmp = a.role_priority.cmp(&b.role_priority);
            if prio_cmp != std::cmp::Ordering::Equal { return prio_cmp; }
            b.is_primary_app.cmp(&a.is_primary_app)
        });

        Ok(candidates.into_iter().next())
    }

    fn row_to_symbol(row: &rusqlite::Row) -> rusqlite::Result<CodeSymbol> {
        Ok(CodeSymbol {
            id: row.get(0)?,
            repo_name: row.get(1)?,
            file_path: row.get(2)?,
            symbol_name: row.get(3)?,
            kind: row.get(4)?,
            signature: row.get(5)?,
            visibility: row.get(6)?,
            start_line: row.get(7)?,
            end_line: row.get(8)?,
            doc_comment: row.get(9)?,
            community_id: row.get(10)?,
            commit_sha: row.get(11)?,
        })
    }

    pub fn upsert_symbol(&self, sym: &CodeSymbol) -> Result<()> {
        self.conn.execute("DELETE FROM symbol_fts WHERE id = ?1", params![sym.id])?;
        self.conn.execute(
            "INSERT INTO code_symbols (id, repo_name, file_path, symbol_name, kind, signature, visibility, start_line, end_line, doc_comment, community_id, commit_sha)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
             ON CONFLICT(id) DO UPDATE SET
                repo_name=excluded.repo_name,
                file_path=excluded.file_path,
                symbol_name=excluded.symbol_name,
                kind=excluded.kind,
                signature=excluded.signature,
                visibility=excluded.visibility,
                start_line=excluded.start_line,
                end_line=excluded.end_line,
                doc_comment=excluded.doc_comment,
                community_id=excluded.community_id,
                commit_sha=excluded.commit_sha",
            params![sym.id, sym.repo_name, sym.file_path, sym.symbol_name, sym.kind, sym.signature, sym.visibility, sym.start_line, sym.end_line, sym.doc_comment, sym.community_id, sym.commit_sha],
        )?;
        
        self.conn.execute(
            "INSERT INTO symbol_fts (id, repo_name, symbol_name, signature, doc_comment, file_path)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![sym.id, sym.repo_name, sym.symbol_name, sym.signature, sym.doc_comment, sym.file_path],
        )?;
        Ok(())
    }

    pub fn get_symbol_by_id(&self, id: &str) -> Result<Option<CodeSymbol>> {
        let mut stmt = self.conn.prepare("SELECT id, repo_name, file_path, symbol_name, kind, signature, visibility, start_line, end_line, doc_comment, community_id, commit_sha FROM code_symbols WHERE id = ?1")?;
        let sym = stmt.query_row(params![id], Self::row_to_symbol).optional()?;
        Ok(sym)
    }

    pub fn get_symbols_by_repo(&self, repo_name: &str) -> Result<Vec<CodeSymbol>> {
        let mut stmt = self.conn.prepare("SELECT id, repo_name, file_path, symbol_name, kind, signature, visibility, start_line, end_line, doc_comment, community_id, commit_sha FROM code_symbols WHERE repo_name = ?1")?;
        let mut res = Vec::new();
        for s in stmt.query_map(params![repo_name], Self::row_to_symbol)? { res.push(s?); }
        Ok(res)
    }

    pub fn get_symbols_by_file(&self, repo_name: &str, file_path: &str) -> Result<Vec<CodeSymbol>> {
        let mut stmt = self.conn.prepare("SELECT id, repo_name, file_path, symbol_name, kind, signature, visibility, start_line, end_line, doc_comment, community_id, commit_sha FROM code_symbols WHERE repo_name = ?1 AND file_path = ?2")?;
        let mut res = Vec::new();
        for s in stmt.query_map(params![repo_name, file_path], Self::row_to_symbol)? { res.push(s?); }
        Ok(res)
    }

    pub fn find_symbols_by_name(&self, name: &str, repo_name: Option<&str>) -> Result<Vec<CodeSymbol>> {
        let mut res = Vec::new();
        if let Some(r) = repo_name {
            let mut stmt = self.conn.prepare("SELECT id, repo_name, file_path, symbol_name, kind, signature, visibility, start_line, end_line, doc_comment, community_id, commit_sha FROM code_symbols WHERE symbol_name = ?1 AND repo_name = ?2")?;
            for s in stmt.query_map(params![name, r], Self::row_to_symbol)? { res.push(s?); }
        } else {
            let mut stmt = self.conn.prepare("SELECT id, repo_name, file_path, symbol_name, kind, signature, visibility, start_line, end_line, doc_comment, community_id, commit_sha FROM code_symbols WHERE symbol_name = ?1")?;
            for s in stmt.query_map(params![name], Self::row_to_symbol)? { res.push(s?); }
        }
        Ok(res)
    }

    pub fn delete_symbols_by_file(&self, repo_name: &str, file_path: &str) -> Result<()> {
        let mut stmt = self.conn.prepare("SELECT id FROM code_symbols WHERE repo_name = ?1 AND file_path = ?2")?;
        let ids: Vec<String> = stmt.query_map(params![repo_name, file_path], |row| row.get(0))?.filter_map(Result::ok).collect();
        for id in ids {
            self.conn.execute("DELETE FROM symbol_fts WHERE id = ?1", params![id])?;
        }
        self.conn.execute("DELETE FROM code_symbols WHERE repo_name = ?1 AND file_path = ?2", params![repo_name, file_path])?;
        Ok(())
    }
    
    pub fn delete_symbols_by_repo(&self, repo_name: &str) -> Result<()> {
        self.conn.execute("DELETE FROM symbol_fts WHERE repo_name = ?1", params![repo_name])?;
        self.conn.execute("DELETE FROM code_symbols WHERE repo_name = ?1", params![repo_name])?;
        Ok(())
    }

    fn row_to_edge(row: &rusqlite::Row) -> rusqlite::Result<CodeEdge> {
        let ct_str: String = row.get(3)?;
        Ok(CodeEdge {
            source_id: row.get(0)?,
            target_id: row.get(1)?,
            edge_type: row.get(2)?,
            confidence_tier: ConfidenceTier::from_str(&ct_str),
            confidence_score: row.get(4)?,
            evidence: row.get(5)?,
        })
    }

    pub fn upsert_edge(&self, edge: &CodeEdge) -> Result<()> {
        self.conn.execute(
            "INSERT INTO code_edges (source_id, target_id, edge_type, confidence_tier, confidence_score, evidence)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(source_id, target_id, edge_type) DO UPDATE SET
                confidence_tier=excluded.confidence_tier,
                confidence_score=excluded.confidence_score,
                evidence=excluded.evidence",
            params![edge.source_id, edge.target_id, edge.edge_type, edge.confidence_tier.as_str(), edge.confidence_score, edge.evidence],
        )?;
        Ok(())
    }

    pub fn get_edges_by_repo(&self, repo_name: &str) -> Result<Vec<CodeEdge>> {
        let mut stmt = self.conn.prepare("SELECT e.source_id, e.target_id, e.edge_type, e.confidence_tier, e.confidence_score, e.evidence FROM code_edges e WHERE e.source_id IN (SELECT id FROM code_symbols WHERE repo_name = ?1) OR e.target_id IN (SELECT id FROM code_symbols WHERE repo_name = ?1) OR e.source_id LIKE ?2")?;
        let mut res = Vec::new();
        let like_pat = format!("{}::%", repo_name);
        for e in stmt.query_map(params![repo_name, like_pat], Self::row_to_edge)? { res.push(e?); }
        Ok(res)
    }

    pub fn get_outgoing_edges(&self, symbol_id: &str) -> Result<Vec<CodeEdge>> {
        let mut stmt = self.conn.prepare("SELECT source_id, target_id, edge_type, confidence_tier, confidence_score, evidence FROM code_edges WHERE source_id = ?1")?;
        let mut res = Vec::new();
        for e in stmt.query_map(params![symbol_id], Self::row_to_edge)? { res.push(e?); }
        Ok(res)
    }

    pub fn get_incoming_edges(&self, symbol_id: &str) -> Result<Vec<CodeEdge>> {
        let mut stmt = self.conn.prepare("SELECT source_id, target_id, edge_type, confidence_tier, confidence_score, evidence FROM code_edges WHERE target_id = ?1")?;
        let mut res = Vec::new();
        for e in stmt.query_map(params![symbol_id], Self::row_to_edge)? { res.push(e?); }
        Ok(res)
    }

    pub fn delete_edges_by_repo(&self, repo_name: &str) -> Result<()> {
        let edges = self.get_edges_by_repo(repo_name)?;
        for e in edges {
            self.conn.execute("DELETE FROM code_edges WHERE source_id = ?1 AND target_id = ?2 AND edge_type = ?3", params![e.source_id, e.target_id, e.edge_type])?;
        }
        Ok(())
    }

    pub fn delete_edges_by_file(&self, repo_name: &str, file_path: &str) -> Result<()> {
        let prefix = format!("{}::{}::%", repo_name, file_path);
        self.conn.execute("DELETE FROM code_edges WHERE source_id LIKE ?1 OR target_id LIKE ?1", params![prefix])?;
        Ok(())
    }

    pub fn delete_edges_by_repo_and_tier(&self, repo_name: &str, tier: ConfidenceTier) -> Result<()> {
        let edges = self.get_edges_by_repo(repo_name)?;
        for e in edges {
            if e.confidence_tier == tier {
                self.conn.execute("DELETE FROM code_edges WHERE source_id = ?1 AND target_id = ?2 AND edge_type = ?3 AND confidence_tier = ?4", params![e.source_id, e.target_id, e.edge_type, tier.as_str()])?;
            }
        }
        Ok(())
    }

    pub fn delete_edges_by_tier(&self, tier: ConfidenceTier) -> Result<()> {
        self.conn.execute("DELETE FROM code_edges WHERE confidence_tier = ?1", params![tier.as_str()])?;
        Ok(())
    }

    pub fn upsert_embedding(&self, symbol_id: &str, repo_name: &str, vector_json: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO symbol_embeddings (symbol_id, repo_name, vector_json) VALUES (?1, ?2, ?3)
             ON CONFLICT(symbol_id) DO UPDATE SET repo_name=excluded.repo_name, vector_json=excluded.vector_json",
            params![symbol_id, repo_name, vector_json],
        )?;
        Ok(())
    }

    pub fn get_embeddings(&self, repo_name: Option<&str>) -> Result<Vec<(String, Vec<f32>)>> {
        let mut res = Vec::new();
        if let Some(r) = repo_name {
            let mut stmt = self.conn.prepare("SELECT symbol_id, vector_json FROM symbol_embeddings WHERE repo_name = ?1")?;
            for row in stmt.query_map(params![r], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))? {
                let (id, json_str) = row?;
                if let Ok(vec) = serde_json::from_str(&json_str) {
                    res.push((id, vec));
                }
            }
        } else {
            let mut stmt = self.conn.prepare("SELECT symbol_id, vector_json FROM symbol_embeddings")?;
            for row in stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))? {
                let (id, json_str) = row?;
                if let Ok(vec) = serde_json::from_str(&json_str) {
                    res.push((id, vec));
                }
            }
        }
        Ok(res)
    }

    pub fn upsert_community(&self, comm: &CommunityRecord) -> Result<()> {
        self.conn.execute(
            "INSERT INTO communities (community_id, repo_name, label, cohesion_score, node_count)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(community_id) DO UPDATE SET
                repo_name=excluded.repo_name,
                label=excluded.label,
                cohesion_score=excluded.cohesion_score,
                node_count=excluded.node_count",
            params![comm.community_id, comm.repo_name, comm.label, comm.cohesion_score, comm.node_count],
        )?;
        Ok(())
    }

    pub fn get_communities(&self, repo_name: &str) -> Result<Vec<CommunityRecord>> {
        let mut stmt = self.conn.prepare("SELECT community_id, repo_name, label, cohesion_score, node_count FROM communities WHERE repo_name = ?1")?;
        let mut res = Vec::new();
        for r in stmt.query_map(params![repo_name], |row| {
            Ok(CommunityRecord {
                community_id: row.get(0)?,
                repo_name: row.get(1)?,
                label: row.get(2)?,
                cohesion_score: row.get(3)?,
                node_count: row.get(4)?,
            })
        })? { res.push(r?); }
        Ok(res)
    }

    pub fn clear_communities_by_repo(&self, repo_name: &str) -> Result<()> {
        self.conn.execute("DELETE FROM communities WHERE repo_name = ?1", params![repo_name])?;
        Ok(())
    }

    pub fn update_symbol_community(&self, symbol_id: &str, community_id: Option<&str>) -> Result<()> {
        self.conn.execute("UPDATE code_symbols SET community_id = ?1 WHERE id = ?2", params![community_id, symbol_id])?;
        Ok(())
    }
    
    pub fn fts_search(&self, query: &str, repo_name: Option<&str>, limit: usize) -> Result<Vec<CodeSymbol>> {
        let tokens: Vec<_> = query.split_whitespace().map(|t| format!("\"{}\"*", t.replace("\"", ""))).collect();
        let fts_query = tokens.join(" OR ");
        
        let mut res = Vec::new();
        
        if let Some(r) = repo_name {
            let mut stmt = self.conn.prepare("SELECT id FROM symbol_fts WHERE repo_name = ?1 AND symbol_fts MATCH ?2 ORDER BY rank LIMIT ?3")?;
            let mut success = true;
            if let Ok(mut rows) = stmt.query(params![r, fts_query, limit]) {
                while let Ok(Some(row)) = rows.next() {
                    let id: String = row.get(0)?;
                    if let Ok(Some(sym)) = self.get_symbol_by_id(&id) {
                        res.push(sym);
                    }
                }
            } else { success = false; }
            
            if !success || res.is_empty() {
                let like_q = format!("%{}%", query);
                let mut stmt = self.conn.prepare("SELECT id FROM code_symbols WHERE repo_name = ?1 AND (symbol_name LIKE ?2 OR signature LIKE ?2 OR doc_comment LIKE ?2 OR file_path LIKE ?2) LIMIT ?3")?;
                let mut rows = stmt.query(params![r, like_q, limit])?;
                while let Ok(Some(row)) = rows.next() {
                    let id: String = row.get(0)?;
                    if let Ok(Some(sym)) = self.get_symbol_by_id(&id) {
                        res.push(sym);
                    }
                }
            }
        } else {
            let mut stmt = self.conn.prepare("SELECT id FROM symbol_fts WHERE symbol_fts MATCH ?1 ORDER BY rank LIMIT ?2")?;
            let mut success = true;
            if let Ok(mut rows) = stmt.query(params![fts_query, limit]) {
                while let Ok(Some(row)) = rows.next() {
                    let id: String = row.get(0)?;
                    if let Ok(Some(sym)) = self.get_symbol_by_id(&id) {
                        res.push(sym);
                    }
                }
            } else { success = false; }
            
            if !success || res.is_empty() {
                let like_q = format!("%{}%", query);
                let mut stmt = self.conn.prepare("SELECT id FROM code_symbols WHERE symbol_name LIKE ?1 OR signature LIKE ?1 OR doc_comment LIKE ?1 OR file_path LIKE ?1 LIMIT ?2")?;
                let mut rows = stmt.query(params![like_q, limit])?;
                while let Ok(Some(row)) = rows.next() {
                    let id: String = row.get(0)?;
                    if let Ok(Some(sym)) = self.get_symbol_by_id(&id) {
                        res.push(sym);
                    }
                }
            }
        }
        
        Ok(res)
    }

    pub fn connection(&self) -> &Connection {
        &self.conn
    }
}
