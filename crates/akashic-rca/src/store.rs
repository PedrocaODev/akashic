use crate::RcaError;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

pub fn current_timestamp_secs() -> i64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowRunRecord {
    pub run_id: String,
    pub jira_key: String,
    pub crash_signature: Option<String>,
    pub status: String,
    pub current_stage: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub context_json: String,
    #[serde(default)]
    pub context_packet_json: Option<String>,
    #[serde(default)]
    pub curation_report_json: Option<String>,
}

pub type SessionRecord = WorkflowRunRecord;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointRecord {
    pub checkpoint_id: i64,
    pub run_id: String,
    pub stage_name: String,
    pub checkpoint_data_json: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StormSignatureRecord {
    pub crash_signature: String,
    pub first_seen_at: i64,
    pub last_seen_at: i64,
    pub hit_count: u32,
    pub active_run_id: String,
    pub incident_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StormGateOutcome {
    Deduplicated { incident_id: String, hit_count: u32 },
    NewRegistered,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IncidentClRecord {
    pub incident_id: String,
    pub resolution_id: String,
    pub repo_name: String,
    pub gerrit_change_id: String,
    pub gerrit_cl_number: i64,
    pub patchset_number: i64,
    pub status: String,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DaemonWatermarkRecord {
    pub stream_name: String,
    pub last_processed_timestamp: i64,
    pub last_processed_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CandidateFindingRecord {
    pub candidate_id: String,
    pub run_id: String,
    #[serde(default)]
    pub agent_id: Option<String>,
    pub storm_candidate_id: Option<String>,
    pub outcome: String,
    pub summary: String,
    pub remediation_proposal: String,
    pub confidence_score: f64,
    pub affected_symbols_json: String,
    pub finding_json: String,
    pub status: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone)]
pub struct StateStore {
    conn: Arc<Mutex<Connection>>,
}

pub type RcaStateStore = StateStore;

impl StateStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, RcaError> {
        let conn = Connection::open(path)?;
        Self::init(conn)
    }

    pub fn open_in_memory() -> Result<Self, RcaError> {
        let conn = Connection::open_in_memory()?;
        Self::init(conn)
    }

    fn init(conn: Connection) -> Result<Self, RcaError> {
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA busy_timeout = 5000;
             PRAGMA foreign_keys = ON;",
        )?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS workflow_runs (
                run_id TEXT PRIMARY KEY,
                jira_key TEXT NOT NULL,
                crash_signature TEXT,
                status TEXT NOT NULL,
                current_stage TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                context_json TEXT NOT NULL,
                context_packet_json TEXT,
                curation_report_json TEXT
            );

            CREATE TABLE IF NOT EXISTS stage_checkpoints (
                checkpoint_id INTEGER PRIMARY KEY AUTOINCREMENT,
                run_id TEXT NOT NULL REFERENCES workflow_runs(run_id),
                stage_name TEXT NOT NULL,
                checkpoint_data_json TEXT NOT NULL,
                created_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS storm_signatures (
                crash_signature TEXT PRIMARY KEY,
                first_seen_at INTEGER NOT NULL,
                last_seen_at INTEGER NOT NULL,
                hit_count INTEGER NOT NULL,
                active_run_id TEXT NOT NULL,
                incident_id TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS incident_cl_map (
                incident_id TEXT PRIMARY KEY,
                resolution_id TEXT NOT NULL,
                repo_name TEXT NOT NULL,
                gerrit_change_id TEXT NOT NULL,
                gerrit_cl_number INTEGER NOT NULL,
                patchset_number INTEGER NOT NULL,
                status TEXT NOT NULL,
                updated_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS daemon_watermarks (
                stream_name TEXT PRIMARY KEY,
                last_processed_timestamp INTEGER NOT NULL,
                last_processed_id TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS candidate_findings (
                candidate_id TEXT NOT NULL,
                run_id TEXT NOT NULL,
                agent_id TEXT,
                storm_candidate_id TEXT,
                outcome TEXT NOT NULL,
                summary TEXT NOT NULL,
                remediation_proposal TEXT NOT NULL,
                confidence_score REAL NOT NULL,
                affected_symbols_json TEXT NOT NULL,
                finding_json TEXT NOT NULL,
                status TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                PRIMARY KEY (candidate_id, run_id)
            );

            CREATE INDEX IF NOT EXISTS idx_checkpoints_run ON stage_checkpoints(run_id, checkpoint_id);
            CREATE INDEX IF NOT EXISTS idx_storm_signatures_seen ON storm_signatures(crash_signature, last_seen_at);
            CREATE INDEX IF NOT EXISTS idx_incident_cl_change ON incident_cl_map(gerrit_change_id);
            CREATE INDEX IF NOT EXISTS idx_candidate_findings_run ON candidate_findings(run_id);
            CREATE INDEX IF NOT EXISTS idx_workflow_runs_jira_key ON workflow_runs(jira_key, updated_at DESC);",
        )?;

        let _ = conn.execute("ALTER TABLE workflow_runs ADD COLUMN context_packet_json TEXT;", []);
        let _ = conn.execute("ALTER TABLE workflow_runs ADD COLUMN curation_report_json TEXT;", []);
        let _ = conn.execute("ALTER TABLE candidate_findings ADD COLUMN agent_id TEXT;", []);

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    // Workflow run operations
    pub fn create_workflow_run(&self, run: &WorkflowRunRecord) -> Result<(), RcaError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| RcaError::Session(format!("Mutex poisoned: {}", e)))?;
        conn.execute(
            "INSERT INTO workflow_runs (run_id, jira_key, crash_signature, status, current_stage, created_at, updated_at, context_json, context_packet_json, curation_report_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            rusqlite::params![
                run.run_id,
                run.jira_key,
                run.crash_signature,
                run.status,
                run.current_stage,
                run.created_at,
                run.updated_at,
                run.context_json,
                run.context_packet_json,
                run.curation_report_json,
            ],
        )?;
        Ok(())
    }

    pub fn update_workflow_run(
        &self,
        run_id: &str,
        current_stage: &str,
        status: &str,
        crash_signature: Option<&str>,
        context_json: &str,
    ) -> Result<(), RcaError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| RcaError::Session(format!("Mutex poisoned: {}", e)))?;
        let updated_at = current_timestamp_secs();
        conn.execute(
            "UPDATE workflow_runs
             SET current_stage = ?1, status = ?2, crash_signature = ?3, context_json = ?4, updated_at = ?5
             WHERE run_id = ?6",
            rusqlite::params![current_stage, status, crash_signature, context_json, updated_at, run_id],
        )?;
        Ok(())
    }

    pub fn save_context_packet(&self, run_id: &str, packet_json: &str) -> Result<(), RcaError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| RcaError::Session(format!("Mutex poisoned: {}", e)))?;
        let updated_at = current_timestamp_secs();
        conn.execute(
            "UPDATE workflow_runs
             SET context_packet_json = ?1, updated_at = ?2
             WHERE run_id = ?3",
            rusqlite::params![packet_json, updated_at, run_id],
        )?;
        Ok(())
    }

    pub fn get_context_packet(&self, run_id: &str) -> Result<Option<String>, RcaError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| RcaError::Session(format!("Mutex poisoned: {}", e)))?;
        let mut stmt = conn.prepare(
            "SELECT context_packet_json
             FROM workflow_runs
             WHERE run_id = ?1",
        )?;
        let mut rows = stmt.query(rusqlite::params![run_id])?;
        if let Some(row) = rows.next()? {
            Ok(row.get(0)?)
        } else {
            Ok(None)
        }
    }

    pub fn save_curation_report(&self, run_id: &str, report_json: &str) -> Result<(), RcaError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| RcaError::Session(format!("Mutex poisoned: {}", e)))?;
        let updated_at = current_timestamp_secs();
        conn.execute(
            "UPDATE workflow_runs
             SET curation_report_json = ?1, updated_at = ?2
             WHERE run_id = ?3",
            rusqlite::params![report_json, updated_at, run_id],
        )?;
        Ok(())
    }

    pub fn get_curation_report(&self, run_id: &str) -> Result<Option<String>, RcaError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| RcaError::Session(format!("Mutex poisoned: {}", e)))?;
        let mut stmt = conn.prepare(
            "SELECT curation_report_json
             FROM workflow_runs
             WHERE run_id = ?1",
        )?;
        let mut rows = stmt.query(rusqlite::params![run_id])?;
        if let Some(row) = rows.next()? {
            Ok(row.get(0)?)
        } else {
            Ok(None)
        }
    }

    pub fn get_workflow_run(&self, run_id: &str) -> Result<Option<WorkflowRunRecord>, RcaError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| RcaError::Session(format!("Mutex poisoned: {}", e)))?;
        let mut stmt = conn.prepare(
            "SELECT run_id, jira_key, crash_signature, status, current_stage, created_at, updated_at, context_json, context_packet_json, curation_report_json
             FROM workflow_runs
             WHERE run_id = ?1",
        )?;
        let mut rows = stmt.query(rusqlite::params![run_id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(WorkflowRunRecord {
                run_id: row.get(0)?,
                jira_key: row.get(1)?,
                crash_signature: row.get(2)?,
                status: row.get(3)?,
                current_stage: row.get(4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
                context_json: row.get(7)?,
                context_packet_json: row.get(8)?,
                curation_report_json: row.get(9)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn list_workflow_runs(&self) -> Result<Vec<WorkflowRunRecord>, RcaError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| RcaError::Session(format!("Mutex poisoned: {}", e)))?;
        let mut stmt = conn.prepare(
            "SELECT run_id, jira_key, crash_signature, status, current_stage, created_at, updated_at, context_json, context_packet_json, curation_report_json
             FROM workflow_runs
             ORDER BY updated_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(WorkflowRunRecord {
                run_id: row.get(0)?,
                jira_key: row.get(1)?,
                crash_signature: row.get(2)?,
                status: row.get(3)?,
                current_stage: row.get(4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
                context_json: row.get(7)?,
                context_packet_json: row.get(8)?,
                curation_report_json: row.get(9)?,
            })
        })?;
        let mut runs = Vec::new();
        for r in rows {
            runs.push(r?);
        }
        Ok(runs)
    }

    pub fn list_workflow_runs_paginated(
        &self,
        status_filter: Option<&str>,
        limit: usize,
    ) -> Result<Vec<WorkflowRunRecord>, RcaError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| RcaError::Session(format!("Mutex poisoned: {}", e)))?;

        let (sql, params_vec): (String, Vec<Box<dyn rusqlite::ToSql>>) = match status_filter {
            Some(status) => (
                "SELECT run_id, jira_key, crash_signature, status, current_stage, created_at, updated_at, context_json, context_packet_json, curation_report_json
                 FROM workflow_runs
                 WHERE UPPER(status) = UPPER(?1)
                 ORDER BY updated_at DESC
                 LIMIT ?2".to_string(),
                vec![Box::new(status.to_string()), Box::new(limit as i64)],
            ),
            None => (
                "SELECT run_id, jira_key, crash_signature, status, current_stage, created_at, updated_at, context_json, context_packet_json, curation_report_json
                 FROM workflow_runs
                 ORDER BY updated_at DESC
                 LIMIT ?1".to_string(),
                vec![Box::new(limit as i64)],
            ),
        };

        let mut stmt = conn.prepare(&sql)?;
        let params_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
        let rows = stmt.query_map(params_refs.as_slice(), |row| {
            Ok(WorkflowRunRecord {
                run_id: row.get(0)?,
                jira_key: row.get(1)?,
                crash_signature: row.get(2)?,
                status: row.get(3)?,
                current_stage: row.get(4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
                context_json: row.get(7)?,
                context_packet_json: row.get(8)?,
                curation_report_json: row.get(9)?,
            })
        })?;
        let mut runs = Vec::new();
        for r in rows {
            runs.push(r?);
        }
        Ok(runs)
    }

    pub fn find_latest_run_for_incident(
        &self,
        incident_id: &str,
    ) -> Result<Option<WorkflowRunRecord>, RcaError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| RcaError::Session(format!("Mutex poisoned: {}", e)))?;

        let jira_key = incident_id.strip_prefix("INC-").unwrap_or(incident_id);

        let mut stmt = conn.prepare(
            "SELECT run_id, jira_key, crash_signature, status, current_stage, created_at, updated_at, context_json, context_packet_json, curation_report_json
             FROM workflow_runs
             WHERE jira_key = ?1 OR jira_key = ?2
             ORDER BY updated_at DESC
             LIMIT 1",
        )?;
        let mut rows = stmt.query(rusqlite::params![jira_key, incident_id])?;
        if let Some(row) = rows.next()? {
            return Ok(Some(WorkflowRunRecord {
                run_id: row.get(0)?,
                jira_key: row.get(1)?,
                crash_signature: row.get(2)?,
                status: row.get(3)?,
                current_stage: row.get(4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
                context_json: row.get(7)?,
                context_packet_json: row.get(8)?,
                curation_report_json: row.get(9)?,
            }));
        }

        // Fallback to checking context_json
        let pattern = format!("%\"incident_id\":\"{}\"%", incident_id);
        let mut stmt2 = conn.prepare(
            "SELECT run_id, jira_key, crash_signature, status, current_stage, created_at, updated_at, context_json, context_packet_json, curation_report_json
             FROM workflow_runs
             WHERE context_json LIKE ?1
             ORDER BY updated_at DESC
             LIMIT 1",
        )?;
        let mut rows2 = stmt2.query(rusqlite::params![pattern])?;
        if let Some(row) = rows2.next()? {
            Ok(Some(WorkflowRunRecord {
                run_id: row.get(0)?,
                jira_key: row.get(1)?,
                crash_signature: row.get(2)?,
                status: row.get(3)?,
                current_stage: row.get(4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
                context_json: row.get(7)?,
                context_packet_json: row.get(8)?,
                curation_report_json: row.get(9)?,
            }))
        } else {
            Ok(None)
        }
    }

    // Checkpoint operations
    pub fn save_checkpoint(
        &self,
        run_id: &str,
        stage_name: &str,
        checkpoint_data_json: &str,
    ) -> Result<i64, RcaError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| RcaError::Session(format!("Mutex poisoned: {}", e)))?;
        let created_at = current_timestamp_secs();
        conn.execute(
            "INSERT INTO stage_checkpoints (run_id, stage_name, checkpoint_data_json, created_at)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![run_id, stage_name, checkpoint_data_json, created_at],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn get_checkpoints(&self, run_id: &str) -> Result<Vec<CheckpointRecord>, RcaError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| RcaError::Session(format!("Mutex poisoned: {}", e)))?;
        let mut stmt = conn.prepare(
            "SELECT checkpoint_id, run_id, stage_name, checkpoint_data_json, created_at
             FROM stage_checkpoints
             WHERE run_id = ?1
             ORDER BY checkpoint_id ASC",
        )?;
        let rows = stmt.query_map(rusqlite::params![run_id], |row| {
            Ok(CheckpointRecord {
                checkpoint_id: row.get(0)?,
                run_id: row.get(1)?,
                stage_name: row.get(2)?,
                checkpoint_data_json: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?;
        let mut checkpoints = Vec::new();
        for c in rows {
            checkpoints.push(c?);
        }
        Ok(checkpoints)
    }

    pub fn get_latest_checkpoint(
        &self,
        run_id: &str,
    ) -> Result<Option<CheckpointRecord>, RcaError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| RcaError::Session(format!("Mutex poisoned: {}", e)))?;
        let mut stmt = conn.prepare(
            "SELECT checkpoint_id, run_id, stage_name, checkpoint_data_json, created_at
             FROM stage_checkpoints
             WHERE run_id = ?1
             ORDER BY checkpoint_id DESC
             LIMIT 1",
        )?;
        let mut rows = stmt.query(rusqlite::params![run_id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(CheckpointRecord {
                checkpoint_id: row.get(0)?,
                run_id: row.get(1)?,
                stage_name: row.get(2)?,
                checkpoint_data_json: row.get(3)?,
                created_at: row.get(4)?,
            }))
        } else {
            Ok(None)
        }
    }

    // Candidate finding operations
    pub fn stage_candidate_finding(&self, record: &CandidateFindingRecord) -> Result<(), RcaError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| RcaError::Session(format!("Mutex poisoned: {}", e)))?;
        conn.execute(
            "INSERT INTO candidate_findings (
                candidate_id, run_id, agent_id, storm_candidate_id, outcome, summary,
                remediation_proposal, confidence_score, affected_symbols_json,
                finding_json, status, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            ON CONFLICT(candidate_id, run_id) DO UPDATE SET
                agent_id = excluded.agent_id,
                storm_candidate_id = excluded.storm_candidate_id,
                outcome = excluded.outcome,
                summary = excluded.summary,
                remediation_proposal = excluded.remediation_proposal,
                confidence_score = excluded.confidence_score,
                affected_symbols_json = excluded.affected_symbols_json,
                finding_json = excluded.finding_json,
                status = excluded.status,
                updated_at = excluded.updated_at",
            rusqlite::params![
                record.candidate_id,
                record.run_id,
                record.agent_id,
                record.storm_candidate_id,
                record.outcome,
                record.summary,
                record.remediation_proposal,
                record.confidence_score,
                record.affected_symbols_json,
                record.finding_json,
                record.status,
                record.created_at,
                record.updated_at,
            ],
        )?;
        Ok(())
    }

    pub fn list_candidate_findings(&self, run_id: &str) -> Result<Vec<CandidateFindingRecord>, RcaError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| RcaError::Session(format!("Mutex poisoned: {}", e)))?;
        let mut stmt = conn.prepare(
            "SELECT candidate_id, run_id, agent_id, storm_candidate_id, outcome, summary,
                    remediation_proposal, confidence_score, affected_symbols_json,
                    finding_json, status, created_at, updated_at
             FROM candidate_findings
             WHERE run_id = ?1
             ORDER BY created_at ASC",
        )?;
        let rows = stmt.query_map(rusqlite::params![run_id], |row| {
            Ok(CandidateFindingRecord {
                candidate_id: row.get(0)?,
                run_id: row.get(1)?,
                agent_id: row.get(2)?,
                storm_candidate_id: row.get(3)?,
                outcome: row.get(4)?,
                summary: row.get(5)?,
                remediation_proposal: row.get(6)?,
                confidence_score: row.get(7)?,
                affected_symbols_json: row.get(8)?,
                finding_json: row.get(9)?,
                status: row.get(10)?,
                created_at: row.get(11)?,
                updated_at: row.get(12)?,
            })
        })?;
        let mut results = Vec::new();
        for r in rows {
            results.push(r?);
        }
        Ok(results)
    }

    pub fn list_candidate_findings_for_runs(
        &self,
        run_ids: &[String],
    ) -> Result<std::collections::HashMap<String, Vec<CandidateFindingRecord>>, RcaError> {
        if run_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }

        let conn = self
            .conn
            .lock()
            .map_err(|e| RcaError::Session(format!("Mutex poisoned: {}", e)))?;

        let placeholders: Vec<String> = (1..=run_ids.len()).map(|i| format!("?{}", i)).collect();
        let sql = format!(
            "SELECT candidate_id, run_id, agent_id, storm_candidate_id, outcome, summary,
                    remediation_proposal, confidence_score, affected_symbols_json,
                    finding_json, status, created_at, updated_at
             FROM candidate_findings
             WHERE run_id IN ({})
             ORDER BY created_at ASC",
            placeholders.join(", ")
        );

        let mut stmt = conn.prepare(&sql)?;
        let params_refs: Vec<&dyn rusqlite::ToSql> = run_ids
            .iter()
            .map(|id| id as &dyn rusqlite::ToSql)
            .collect();

        let rows = stmt.query_map(params_refs.as_slice(), |row| {
            Ok(CandidateFindingRecord {
                candidate_id: row.get(0)?,
                run_id: row.get(1)?,
                agent_id: row.get(2)?,
                storm_candidate_id: row.get(3)?,
                outcome: row.get(4)?,
                summary: row.get(5)?,
                remediation_proposal: row.get(6)?,
                confidence_score: row.get(7)?,
                affected_symbols_json: row.get(8)?,
                finding_json: row.get(9)?,
                status: row.get(10)?,
                created_at: row.get(11)?,
                updated_at: row.get(12)?,
            })
        })?;

        let mut map: std::collections::HashMap<String, Vec<CandidateFindingRecord>> =
            std::collections::HashMap::new();
        for r in rows {
            let record = r?;
            map.entry(record.run_id.clone())
                .or_default()
                .push(record);
        }
        Ok(map)
    }

    pub fn update_candidate_finding_status(&self, candidate_id: &str, status: &str) -> Result<(), RcaError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| RcaError::Session(format!("Mutex poisoned: {}", e)))?;
        let now = current_timestamp_secs();
        conn.execute(
            "UPDATE candidate_findings SET status = ?1, updated_at = ?2 WHERE candidate_id = ?3",
            rusqlite::params![status, now, candidate_id],
        )?;
        Ok(())
    }

    pub fn adjudicate_candidate_findings(&self, run_id: &str, winning_candidate_id: &str) -> Result<(), RcaError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| RcaError::Session(format!("Mutex poisoned: {}", e)))?;
        let now = current_timestamp_secs();
        conn.execute(
            "UPDATE candidate_findings
             SET status = CASE WHEN candidate_id = ?1 THEN 'ACCEPTED' ELSE 'REJECTED' END,
                 updated_at = ?2
             WHERE run_id = ?3",
            rusqlite::params![winning_candidate_id, now, run_id],
        )?;
        Ok(())
    }

    // Storm signature operations
    pub fn check_or_register_storm_signature(
        &self,
        signature: &str,
        run_id: &str,
        incident_id: &str,
    ) -> Result<StormGateOutcome, RcaError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| RcaError::Session(format!("Mutex poisoned: {}", e)))?;
        let now = current_timestamp_secs();

        let mut stmt = conn.prepare(
            "SELECT crash_signature, first_seen_at, last_seen_at, hit_count, active_run_id, incident_id
             FROM storm_signatures
             WHERE crash_signature = ?1",
        )?;
        let mut rows = stmt.query(rusqlite::params![signature])?;
        if let Some(row) = rows.next()? {
            let existing_incident_id: String = row.get(5)?;
            let prev_count: u32 = row.get(3)?;
            let new_count = prev_count + 1;
            conn.execute(
                "UPDATE storm_signatures
                 SET hit_count = ?1, last_seen_at = ?2
                 WHERE crash_signature = ?3",
                rusqlite::params![new_count, now, signature],
            )?;
            Ok(StormGateOutcome::Deduplicated {
                incident_id: existing_incident_id,
                hit_count: new_count,
            })
        } else {
            conn.execute(
                "INSERT INTO storm_signatures (crash_signature, first_seen_at, last_seen_at, hit_count, active_run_id, incident_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![signature, now, now, 1u32, run_id, incident_id],
            )?;
            Ok(StormGateOutcome::NewRegistered)
        }
    }

    pub fn check_storm_gate(
        &self,
        signature: &str,
        window_secs: i64,
    ) -> Result<Option<StormSignatureRecord>, RcaError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| RcaError::Session(format!("Mutex poisoned: {}", e)))?;
        let now = current_timestamp_secs();
        let cutoff = now - window_secs;
        let mut stmt = conn.prepare(
            "SELECT crash_signature, first_seen_at, last_seen_at, hit_count, active_run_id, incident_id
             FROM storm_signatures
             WHERE crash_signature = ?1 AND last_seen_at >= ?2",
        )?;
        let mut rows = stmt.query(rusqlite::params![signature, cutoff])?;
        if let Some(row) = rows.next()? {
            Ok(Some(StormSignatureRecord {
                crash_signature: row.get(0)?,
                first_seen_at: row.get(1)?,
                last_seen_at: row.get(2)?,
                hit_count: row.get(3)?,
                active_run_id: row.get(4)?,
                incident_id: row.get(5)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn register_storm_signature(&self, record: &StormSignatureRecord) -> Result<(), RcaError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| RcaError::Session(format!("Mutex poisoned: {}", e)))?;
        conn.execute(
            "INSERT INTO storm_signatures (crash_signature, first_seen_at, last_seen_at, hit_count, active_run_id, incident_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(crash_signature) DO UPDATE SET
                last_seen_at = excluded.last_seen_at,
                hit_count = excluded.hit_count,
                active_run_id = excluded.active_run_id,
                incident_id = excluded.incident_id",
            rusqlite::params![
                record.crash_signature,
                record.first_seen_at,
                record.last_seen_at,
                record.hit_count,
                record.active_run_id,
                record.incident_id,
            ],
        )?;
        Ok(())
    }

    pub fn increment_storm_signature_hit(&self, signature: &str) -> Result<u32, RcaError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| RcaError::Session(format!("Mutex poisoned: {}", e)))?;
        let now = current_timestamp_secs();
        let mut stmt = conn.prepare(
            "SELECT hit_count FROM storm_signatures WHERE crash_signature = ?1",
        )?;
        let mut rows = stmt.query(rusqlite::params![signature])?;
        if let Some(row) = rows.next()? {
            let count: u32 = row.get(0)?;
            let new_count = count + 1;
            conn.execute(
                "UPDATE storm_signatures SET hit_count = ?1, last_seen_at = ?2 WHERE crash_signature = ?3",
                rusqlite::params![new_count, now, signature],
            )?;
            Ok(new_count)
        } else {
            Err(RcaError::Session(format!("Storm signature not found: {}", signature)))
        }
    }

    pub fn update_storm_active_run(&self, crash_signature: &str, new_run_id: &str) -> Result<(), RcaError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| RcaError::Session(format!("Mutex poisoned: {}", e)))?;
        let now = current_timestamp_secs();
        conn.execute(
            "UPDATE storm_signatures SET active_run_id = ?1, last_seen_at = ?2 WHERE crash_signature = ?3",
            rusqlite::params![new_run_id, now, crash_signature],
        )?;
        Ok(())
    }

    // Incident CL map operations
    pub fn upsert_incident_cl(&self, record: &IncidentClRecord) -> Result<(), RcaError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| RcaError::Session(format!("Mutex poisoned: {}", e)))?;
        conn.execute(
            "INSERT INTO incident_cl_map (incident_id, resolution_id, repo_name, gerrit_change_id, gerrit_cl_number, patchset_number, status, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(incident_id) DO UPDATE SET
                resolution_id = excluded.resolution_id,
                repo_name = excluded.repo_name,
                gerrit_change_id = excluded.gerrit_change_id,
                gerrit_cl_number = excluded.gerrit_cl_number,
                patchset_number = excluded.patchset_number,
                status = excluded.status,
                updated_at = excluded.updated_at",
            rusqlite::params![
                record.incident_id,
                record.resolution_id,
                record.repo_name,
                record.gerrit_change_id,
                record.gerrit_cl_number,
                record.patchset_number,
                record.status,
                record.updated_at,
            ],
        )?;
        Ok(())
    }

    pub fn get_incident_cl(&self, incident_or_change_id: &str) -> Result<Option<IncidentClRecord>, RcaError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| RcaError::Session(format!("Mutex poisoned: {}", e)))?;
        let mut stmt = conn.prepare(
            "SELECT incident_id, resolution_id, repo_name, gerrit_change_id, gerrit_cl_number, patchset_number, status, updated_at
             FROM incident_cl_map
             WHERE incident_id = ?1 OR gerrit_change_id = ?1",
        )?;
        let mut rows = stmt.query(rusqlite::params![incident_or_change_id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(IncidentClRecord {
                incident_id: row.get(0)?,
                resolution_id: row.get(1)?,
                repo_name: row.get(2)?,
                gerrit_change_id: row.get(3)?,
                gerrit_cl_number: row.get(4)?,
                patchset_number: row.get(5)?,
                status: row.get(6)?,
                updated_at: row.get(7)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn get_incident_cl_by_change_id(&self, change_id: &str) -> Result<Option<IncidentClRecord>, RcaError> {
        self.get_incident_cl(change_id)
    }

    pub fn update_incident_cl_status(&self, incident_or_change_id: &str, status: &str) -> Result<(), RcaError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| RcaError::Session(format!("Mutex poisoned: {}", e)))?;
        let now = current_timestamp_secs();
        conn.execute(
            "UPDATE incident_cl_map SET status = ?1, updated_at = ?2 WHERE incident_id = ?3 OR gerrit_change_id = ?3",
            rusqlite::params![status, now, incident_or_change_id],
        )?;
        Ok(())
    }

    // Daemon watermarks
    pub fn get_watermark(&self, stream_name: &str) -> Result<Option<DaemonWatermarkRecord>, RcaError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| RcaError::Session(format!("Mutex poisoned: {}", e)))?;
        let mut stmt = conn.prepare(
            "SELECT stream_name, last_processed_timestamp, last_processed_id
             FROM daemon_watermarks
             WHERE stream_name = ?1",
        )?;
        let mut rows = stmt.query(rusqlite::params![stream_name])?;
        if let Some(row) = rows.next()? {
            Ok(Some(DaemonWatermarkRecord {
                stream_name: row.get(0)?,
                last_processed_timestamp: row.get(1)?,
                last_processed_id: row.get(2)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn set_watermark(
        &self,
        stream_name: &str,
        last_processed_timestamp: i64,
        last_processed_id: &str,
    ) -> Result<(), RcaError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| RcaError::Session(format!("Mutex poisoned: {}", e)))?;
        conn.execute(
            "INSERT INTO daemon_watermarks (stream_name, last_processed_timestamp, last_processed_id)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(stream_name) DO UPDATE SET
                last_processed_timestamp = excluded.last_processed_timestamp,
                last_processed_id = excluded.last_processed_id",
            rusqlite::params![stream_name, last_processed_timestamp, last_processed_id],
        )?;
        Ok(())
    }

    // Backwards-compatible session helper aliases
    pub fn create_session(&self, session: &SessionRecord) -> Result<(), RcaError> {
        self.create_workflow_run(session)
    }

    pub fn update_session(
        &self,
        session_id: &str,
        stage: &str,
        status: &str,
        crash_signature: Option<&str>,
        context_json: &str,
    ) -> Result<(), RcaError> {
        self.update_workflow_run(session_id, stage, status, crash_signature, context_json)
    }

    pub fn get_session(&self, session_id: &str) -> Result<Option<SessionRecord>, RcaError> {
        self.get_workflow_run(session_id)
    }

    pub fn list_sessions(&self) -> Result<Vec<SessionRecord>, RcaError> {
        self.list_workflow_runs()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_candidate_findings_composite_pk_and_agent_id() {
        let store = StateStore::open_in_memory().unwrap();

        let finding1 = CandidateFindingRecord {
            candidate_id: "cand-1".to_string(),
            run_id: "run-A".to_string(),
            agent_id: Some("agent-alpha".to_string()),
            storm_candidate_id: None,
            outcome: "RESOLVED".to_string(),
            summary: "Null check issue in CameraMode".to_string(),
            remediation_proposal: "Add null check".to_string(),
            confidence_score: 0.95,
            affected_symbols_json: "[]".to_string(),
            finding_json: "{}".to_string(),
            status: "ACCEPTED".to_string(),
            created_at: 100,
            updated_at: 100,
        };

        // Same candidate_id, different run_id should succeed due to composite PK (candidate_id, run_id)
        let finding2 = CandidateFindingRecord {
            candidate_id: "cand-1".to_string(),
            run_id: "run-B".to_string(),
            agent_id: Some("agent-beta".to_string()),
            storm_candidate_id: None,
            outcome: "RESOLVED".to_string(),
            summary: "Secondary run candidate".to_string(),
            remediation_proposal: "Add guard clause".to_string(),
            confidence_score: 0.88,
            affected_symbols_json: "[]".to_string(),
            finding_json: "{}".to_string(),
            status: "PENDING".to_string(),
            created_at: 200,
            updated_at: 200,
        };

        store.stage_candidate_finding(&finding1).unwrap();
        store.stage_candidate_finding(&finding2).unwrap();

        let run_a_findings = store.list_candidate_findings("run-A").unwrap();
        assert_eq!(run_a_findings.len(), 1);
        assert_eq!(run_a_findings[0].candidate_id, "cand-1");
        assert_eq!(run_a_findings[0].run_id, "run-A");
        assert_eq!(run_a_findings[0].agent_id.as_deref(), Some("agent-alpha"));

        let run_b_findings = store.list_candidate_findings("run-B").unwrap();
        assert_eq!(run_b_findings.len(), 1);
        assert_eq!(run_b_findings[0].candidate_id, "cand-1");
        assert_eq!(run_b_findings[0].run_id, "run-B");
        assert_eq!(run_b_findings[0].agent_id.as_deref(), Some("agent-beta"));

        // Upserting the same (candidate_id, run_id) updates on conflict
        let mut finding1_updated = finding1.clone();
        finding1_updated.agent_id = Some("agent-gamma".to_string());
        finding1_updated.confidence_score = 0.99;
        store.stage_candidate_finding(&finding1_updated).unwrap();

        let run_a_updated = store.list_candidate_findings("run-A").unwrap();
        assert_eq!(run_a_updated.len(), 1);
        assert_eq!(run_a_updated[0].agent_id.as_deref(), Some("agent-gamma"));
        assert_eq!(run_a_updated[0].confidence_score, 0.99);
    }

    #[test]
    fn test_list_candidate_findings_for_runs_batch() {
        let store = StateStore::open_in_memory().unwrap();

        let f1 = CandidateFindingRecord {
            candidate_id: "c1".to_string(),
            run_id: "run-1".to_string(),
            agent_id: Some("agent-1".to_string()),
            storm_candidate_id: None,
            outcome: "RESOLVED".to_string(),
            summary: "Summary 1".to_string(),
            remediation_proposal: "Fix 1".to_string(),
            confidence_score: 0.9,
            affected_symbols_json: "[]".to_string(),
            finding_json: "{}".to_string(),
            status: "ACCEPTED".to_string(),
            created_at: 10,
            updated_at: 10,
        };

        let f2 = CandidateFindingRecord {
            candidate_id: "c2".to_string(),
            run_id: "run-1".to_string(),
            agent_id: Some("agent-2".to_string()),
            storm_candidate_id: None,
            outcome: "RESOLVED".to_string(),
            summary: "Summary 2".to_string(),
            remediation_proposal: "Fix 2".to_string(),
            confidence_score: 0.85,
            affected_symbols_json: "[]".to_string(),
            finding_json: "{}".to_string(),
            status: "REJECTED".to_string(),
            created_at: 20,
            updated_at: 20,
        };

        let f3 = CandidateFindingRecord {
            candidate_id: "c3".to_string(),
            run_id: "run-2".to_string(),
            agent_id: None,
            storm_candidate_id: None,
            outcome: "INCONCLUSIVE".to_string(),
            summary: "Summary 3".to_string(),
            remediation_proposal: "Fix 3".to_string(),
            confidence_score: 0.5,
            affected_symbols_json: "[]".to_string(),
            finding_json: "{}".to_string(),
            status: "PENDING".to_string(),
            created_at: 30,
            updated_at: 30,
        };

        store.stage_candidate_finding(&f1).unwrap();
        store.stage_candidate_finding(&f2).unwrap();
        store.stage_candidate_finding(&f3).unwrap();

        // Empty run_ids returns empty map
        let empty_result = store.list_candidate_findings_for_runs(&[]).unwrap();
        assert!(empty_result.is_empty());

        // Batch query for run-1 and run-2
        let result = store
            .list_candidate_findings_for_runs(&["run-1".to_string(), "run-2".to_string()])
            .unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result.get("run-1").unwrap().len(), 2);
        assert_eq!(result.get("run-2").unwrap().len(), 1);
        assert_eq!(result.get("run-2").unwrap()[0].agent_id, None);
    }

    #[test]
    fn test_curation_report_persistence() {
        let store = StateStore::open_in_memory().unwrap();

        let run = WorkflowRunRecord {
            run_id: "run-curation-01".to_string(),
            jira_key: "MOTOCAM-1234".to_string(),
            crash_signature: Some("SIG-1".to_string()),
            status: "COMPLETED".to_string(),
            current_stage: "Stage6".to_string(),
            created_at: 100,
            updated_at: 100,
            context_json: "{}".to_string(),
            context_packet_json: None,
            curation_report_json: None,
        };
        store.create_workflow_run(&run).unwrap();

        let initial = store.get_curation_report("run-curation-01").unwrap();
        assert_eq!(initial, None);

        let report_sample = r#"{"verdict":"Approved","issues":[],"deduplicated":false}"#;
        store
            .save_curation_report("run-curation-01", report_sample)
            .unwrap();

        let retrieved = store.get_curation_report("run-curation-01").unwrap();
        assert_eq!(retrieved.as_deref(), Some(report_sample));

        let updated_run = store.get_workflow_run("run-curation-01").unwrap().unwrap();
        assert_eq!(
            updated_run.curation_report_json.as_deref(),
            Some(report_sample)
        );
    }

    #[test]
    fn test_list_workflow_runs_paginated_and_find_latest_run() {
        let store = StateStore::open_in_memory().unwrap();

        let run1 = WorkflowRunRecord {
            run_id: "run-p-01".to_string(),
            jira_key: "MOTOCAM-5001".to_string(),
            crash_signature: None,
            status: "PENDING".to_string(),
            current_stage: "Stage1".to_string(),
            created_at: 100,
            updated_at: 100,
            context_json: r#"{"incident_id":"INC-MOTOCAM-5001"}"#.to_string(),
            context_packet_json: None,
            curation_report_json: None,
        };

        let run2 = WorkflowRunRecord {
            run_id: "run-p-02".to_string(),
            jira_key: "MOTOCAM-5001".to_string(),
            crash_signature: None,
            status: "COMPLETED".to_string(),
            current_stage: "Stage6".to_string(),
            created_at: 200,
            updated_at: 200,
            context_json: r#"{"incident_id":"INC-MOTOCAM-5001"}"#.to_string(),
            context_packet_json: None,
            curation_report_json: None,
        };

        let run3 = WorkflowRunRecord {
            run_id: "run-p-03".to_string(),
            jira_key: "MOTOCAM-6002".to_string(),
            crash_signature: None,
            status: "FAILED".to_string(),
            current_stage: "Stage3".to_string(),
            created_at: 300,
            updated_at: 300,
            context_json: r#"{"incident_id":"INC-MOTOCAM-6002"}"#.to_string(),
            context_packet_json: None,
            curation_report_json: None,
        };

        store.create_workflow_run(&run1).unwrap();
        store.create_workflow_run(&run2).unwrap();
        store.create_workflow_run(&run3).unwrap();

        // Test list_workflow_runs_paginated with no status filter and limit 2
        let paginated_all = store.list_workflow_runs_paginated(None, 2).unwrap();
        assert_eq!(paginated_all.len(), 2);
        assert_eq!(paginated_all[0].run_id, "run-p-03");
        assert_eq!(paginated_all[1].run_id, "run-p-02");

        // Test status filtering (case-insensitive)
        let completed = store
            .list_workflow_runs_paginated(Some("completed"), 10)
            .unwrap();
        assert_eq!(completed.len(), 1);
        assert_eq!(completed[0].run_id, "run-p-02");

        // Test find_latest_run_for_incident:
        // 1. With "INC-" prefix
        let latest_inc = store
            .find_latest_run_for_incident("INC-MOTOCAM-5001")
            .unwrap()
            .unwrap();
        assert_eq!(latest_inc.run_id, "run-p-02");

        // 2. Direct jira_key without prefix
        let latest_jira = store
            .find_latest_run_for_incident("MOTOCAM-5001")
            .unwrap()
            .unwrap();
        assert_eq!(latest_jira.run_id, "run-p-02");

        // 3. Fallback to context_json matching
        let run_fallback = WorkflowRunRecord {
            run_id: "run-p-04".to_string(),
            jira_key: "OTHER-999".to_string(),
            crash_signature: None,
            status: "COMPLETED".to_string(),
            current_stage: "Stage6".to_string(),
            created_at: 400,
            updated_at: 400,
            context_json: r#"{"incident_id":"SPECIAL-INC-777"}"#.to_string(),
            context_packet_json: None,
            curation_report_json: None,
        };
        store.create_workflow_run(&run_fallback).unwrap();

        let latest_fallback = store
            .find_latest_run_for_incident("SPECIAL-INC-777")
            .unwrap()
            .unwrap();
        assert_eq!(latest_fallback.run_id, "run-p-04");
    }
}
