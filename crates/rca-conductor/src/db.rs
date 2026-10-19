use crate::ConductorError;
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
pub struct SessionRecord {
    pub session_id: String,
    pub jira_key: String,
    pub crash_signature: Option<String>,
    pub stage: String,
    pub status: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub context_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointRecord {
    pub checkpoint_id: i64,
    pub session_id: String,
    pub stage: String,
    pub state_json: String,
    pub recorded_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveIncident {
    pub incident_id: String,
    pub jira_key: String,
    pub crash_signature: String,
    pub session_id: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone)]
pub struct StateStore {
    conn: Arc<Mutex<Connection>>,
}

impl StateStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, ConductorError> {
        let conn = Connection::open(path)?;
        Self::init(conn)
    }

    pub fn open_in_memory() -> Result<Self, ConductorError> {
        let conn = Connection::open_in_memory()?;
        Self::init(conn)
    }

    fn init(conn: Connection) -> Result<Self, ConductorError> {
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA busy_timeout = 5000;
             PRAGMA foreign_keys = ON;",
        )?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS rca_sessions (
                session_id TEXT PRIMARY KEY,
                jira_key TEXT NOT NULL,
                crash_signature TEXT,
                stage TEXT NOT NULL,
                status TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                context_json TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS rca_checkpoints (
                checkpoint_id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL REFERENCES rca_sessions(session_id),
                stage TEXT NOT NULL,
                state_json TEXT NOT NULL,
                recorded_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS rca_active_incidents (
                incident_id TEXT PRIMARY KEY,
                jira_key TEXT NOT NULL,
                crash_signature TEXT NOT NULL UNIQUE,
                session_id TEXT NOT NULL REFERENCES rca_sessions(session_id),
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_active_incidents_sig ON rca_active_incidents(crash_signature, updated_at);
            CREATE INDEX IF NOT EXISTS idx_checkpoints_session ON rca_checkpoints(session_id, checkpoint_id);",
        )?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    pub fn create_session(&self, session: &SessionRecord) -> Result<(), ConductorError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| ConductorError::Session(format!("Mutex poisoned: {}", e)))?;
        conn.execute(
            "INSERT INTO rca_sessions (session_id, jira_key, crash_signature, stage, status, created_at, updated_at, context_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                session.session_id,
                session.jira_key,
                session.crash_signature,
                session.stage,
                session.status,
                session.created_at,
                session.updated_at,
                session.context_json,
            ],
        )?;
        Ok(())
    }

    pub fn update_session(
        &self,
        session_id: &str,
        stage: &str,
        status: &str,
        crash_signature: Option<&str>,
        context_json: &str,
    ) -> Result<(), ConductorError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| ConductorError::Session(format!("Mutex poisoned: {}", e)))?;
        let updated_at = current_timestamp_secs();
        conn.execute(
            "UPDATE rca_sessions
             SET stage = ?1, status = ?2, crash_signature = ?3, context_json = ?4, updated_at = ?5
             WHERE session_id = ?6",
            rusqlite::params![stage, status, crash_signature, context_json, updated_at, session_id],
        )?;
        Ok(())
    }

    pub fn get_session(&self, session_id: &str) -> Result<Option<SessionRecord>, ConductorError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| ConductorError::Session(format!("Mutex poisoned: {}", e)))?;
        let mut stmt = conn.prepare(
            "SELECT session_id, jira_key, crash_signature, stage, status, created_at, updated_at, context_json
             FROM rca_sessions
             WHERE session_id = ?1",
        )?;
        let mut rows = stmt.query(rusqlite::params![session_id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(SessionRecord {
                session_id: row.get(0)?,
                jira_key: row.get(1)?,
                crash_signature: row.get(2)?,
                stage: row.get(3)?,
                status: row.get(4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
                context_json: row.get(7)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn list_sessions(&self) -> Result<Vec<SessionRecord>, ConductorError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| ConductorError::Session(format!("Mutex poisoned: {}", e)))?;
        let mut stmt = conn.prepare(
            "SELECT session_id, jira_key, crash_signature, stage, status, created_at, updated_at, context_json
             FROM rca_sessions
             ORDER BY updated_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(SessionRecord {
                session_id: row.get(0)?,
                jira_key: row.get(1)?,
                crash_signature: row.get(2)?,
                stage: row.get(3)?,
                status: row.get(4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
                context_json: row.get(7)?,
            })
        })?;
        let mut sessions = Vec::new();
        for s in rows {
            sessions.push(s?);
        }
        Ok(sessions)
    }

    pub fn save_checkpoint(
        &self,
        session_id: &str,
        stage: &str,
        state_json: &str,
    ) -> Result<i64, ConductorError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| ConductorError::Session(format!("Mutex poisoned: {}", e)))?;
        let recorded_at = current_timestamp_secs();
        conn.execute(
            "INSERT INTO rca_checkpoints (session_id, stage, state_json, recorded_at)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![session_id, stage, state_json, recorded_at],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn get_checkpoints(&self, session_id: &str) -> Result<Vec<CheckpointRecord>, ConductorError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| ConductorError::Session(format!("Mutex poisoned: {}", e)))?;
        let mut stmt = conn.prepare(
            "SELECT checkpoint_id, session_id, stage, state_json, recorded_at
             FROM rca_checkpoints
             WHERE session_id = ?1
             ORDER BY checkpoint_id ASC",
        )?;
        let rows = stmt.query_map(rusqlite::params![session_id], |row| {
            Ok(CheckpointRecord {
                checkpoint_id: row.get(0)?,
                session_id: row.get(1)?,
                stage: row.get(2)?,
                state_json: row.get(3)?,
                recorded_at: row.get(4)?,
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
        session_id: &str,
    ) -> Result<Option<CheckpointRecord>, ConductorError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| ConductorError::Session(format!("Mutex poisoned: {}", e)))?;
        let mut stmt = conn.prepare(
            "SELECT checkpoint_id, session_id, stage, state_json, recorded_at
             FROM rca_checkpoints
             WHERE session_id = ?1
             ORDER BY checkpoint_id DESC
             LIMIT 1",
        )?;
        let mut rows = stmt.query(rusqlite::params![session_id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(CheckpointRecord {
                checkpoint_id: row.get(0)?,
                session_id: row.get(1)?,
                stage: row.get(2)?,
                state_json: row.get(3)?,
                recorded_at: row.get(4)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn check_storm_gate(
        &self,
        crash_signature: &str,
        window_secs: i64,
    ) -> Result<Option<ActiveIncident>, ConductorError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| ConductorError::Session(format!("Mutex poisoned: {}", e)))?;
        let now = current_timestamp_secs();
        let cutoff = now - window_secs;
        let mut stmt = conn.prepare(
            "SELECT incident_id, jira_key, crash_signature, session_id, created_at, updated_at
             FROM rca_active_incidents
             WHERE crash_signature = ?1 AND updated_at >= ?2
             ORDER BY updated_at DESC
             LIMIT 1",
        )?;
        let mut rows = stmt.query(rusqlite::params![crash_signature, cutoff])?;
        if let Some(row) = rows.next()? {
            Ok(Some(ActiveIncident {
                incident_id: row.get(0)?,
                jira_key: row.get(1)?,
                crash_signature: row.get(2)?,
                session_id: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn register_active_incident(&self, incident: &ActiveIncident) -> Result<(), ConductorError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| ConductorError::Session(format!("Mutex poisoned: {}", e)))?;
        conn.execute(
            "INSERT INTO rca_active_incidents (incident_id, jira_key, crash_signature, session_id, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(crash_signature) DO UPDATE SET
                incident_id = excluded.incident_id,
                jira_key = excluded.jira_key,
                session_id = excluded.session_id,
                updated_at = excluded.updated_at",
            rusqlite::params![
                incident.incident_id,
                incident.jira_key,
                incident.crash_signature,
                incident.session_id,
                incident.created_at,
                incident.updated_at,
            ],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_db_path() -> (std::path::PathBuf, impl Drop) {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let id = COUNTER.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!("rca_test_db_{}_{}.db", std::process::id(), id));
        struct Cleanup(std::path::PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.0);
            }
        }
        let cleanup = Cleanup(path.clone());
        (path, cleanup)
    }

    #[test]
    fn test_sqlite_wal_pragmas() {
        let (path, _clean) = temp_db_path();
        let store = StateStore::open(&path).expect("Should open disk db");
        let conn = store.conn.lock().unwrap();

        let journal_mode: String = conn
            .query_row("PRAGMA journal_mode;", [], |r| r.get(0))
            .unwrap();
        assert_eq!(journal_mode.to_lowercase(), "wal");

        let busy_timeout: i64 = conn
            .query_row("PRAGMA busy_timeout;", [], |r| r.get(0))
            .unwrap();
        assert_eq!(busy_timeout, 5000);

        let synchronous: i64 = conn
            .query_row("PRAGMA synchronous;", [], |r| r.get(0))
            .unwrap();
        // NORMAL synchronous is code 1
        assert_eq!(synchronous, 1);
    }

    #[test]
    fn test_session_crud() {
        let store = StateStore::open_in_memory().expect("Should open in-memory db");

        let session = SessionRecord {
            session_id: "sess-001".to_string(),
            jira_key: "MOTOCAM-100".to_string(),
            crash_signature: None,
            stage: "CREATED".to_string(),
            status: "IN_PROGRESS".to_string(),
            created_at: 1000,
            updated_at: 1000,
            context_json: "{}".to_string(),
        };

        store.create_session(&session).unwrap();

        let fetched = store.get_session("sess-001").unwrap().expect("Should exist");
        assert_eq!(fetched.session_id, "sess-001");
        assert_eq!(fetched.stage, "CREATED");

        // Update session
        store
            .update_session(
                "sess-001",
                "TRIAGED",
                "IN_PROGRESS",
                Some("sig123"),
                "{\"updated\":true}",
            )
            .unwrap();

        let updated = store.get_session("sess-001").unwrap().expect("Should exist");
        assert_eq!(updated.stage, "TRIAGED");
        assert_eq!(updated.crash_signature.as_deref(), Some("sig123"));
        assert_eq!(updated.context_json, "{\"updated\":true}");

        // List sessions
        let list = store.list_sessions().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].session_id, "sess-001");
    }

    #[test]
    fn test_checkpoint_crud() {
        let store = StateStore::open_in_memory().expect("Should open in-memory db");

        let session = SessionRecord {
            session_id: "sess-002".to_string(),
            jira_key: "MOTOCAM-101".to_string(),
            crash_signature: None,
            stage: "CREATED".to_string(),
            status: "IN_PROGRESS".to_string(),
            created_at: 1000,
            updated_at: 1000,
            context_json: "{}".to_string(),
        };
        store.create_session(&session).unwrap();

        let cp1_id = store
            .save_checkpoint("sess-002", "TRIAGED", "{\"stage\":1}")
            .unwrap();
        let cp2_id = store
            .save_checkpoint("sess-002", "SANITIZED", "{\"stage\":2}")
            .unwrap();

        assert!(cp2_id > cp1_id);

        let cps = store.get_checkpoints("sess-002").unwrap();
        assert_eq!(cps.len(), 2);
        assert_eq!(cps[0].stage, "TRIAGED");
        assert_eq!(cps[1].stage, "SANITIZED");

        let latest = store
            .get_latest_checkpoint("sess-002")
            .unwrap()
            .expect("Should have latest");
        assert_eq!(latest.checkpoint_id, cp2_id);
        assert_eq!(latest.stage, "SANITIZED");
    }

    #[test]
    fn test_storm_gate_deduplication_and_upsert() {
        let store = StateStore::open_in_memory().expect("Should open in-memory db");

        let session = SessionRecord {
            session_id: "sess-003".to_string(),
            jira_key: "MOTOCAM-102".to_string(),
            crash_signature: Some("deadbeefsig".to_string()),
            stage: "TRIAGED".to_string(),
            status: "IN_PROGRESS".to_string(),
            created_at: current_timestamp_secs(),
            updated_at: current_timestamp_secs(),
            context_json: "{}".to_string(),
        };
        store.create_session(&session).unwrap();

        // Initially no active incident
        let hit = store.check_storm_gate("deadbeefsig", 86400).unwrap();
        assert!(hit.is_none());

        let incident = ActiveIncident {
            incident_id: "INC-MOTOCAM-102".to_string(),
            jira_key: "MOTOCAM-102".to_string(),
            crash_signature: "deadbeefsig".to_string(),
            session_id: "sess-003".to_string(),
            created_at: current_timestamp_secs(),
            updated_at: current_timestamp_secs(),
        };

        store.register_active_incident(&incident).unwrap();

        // Query storm gate within 24h
        let hit2 = store.check_storm_gate("deadbeefsig", 86400).unwrap();
        assert!(hit2.is_some());
        let found = hit2.unwrap();
        assert_eq!(found.incident_id, "INC-MOTOCAM-102");
        assert_eq!(found.crash_signature, "deadbeefsig");

        // Query storm gate with other signature
        let hit3 = store.check_storm_gate("other_sig", 86400).unwrap();
        assert!(hit3.is_none());

        // Upsert incident with updated session
        let updated_incident = ActiveIncident {
            incident_id: "INC-MOTOCAM-102-V2".to_string(),
            jira_key: "MOTOCAM-102".to_string(),
            crash_signature: "deadbeefsig".to_string(),
            session_id: "sess-003".to_string(),
            created_at: incident.created_at,
            updated_at: current_timestamp_secs() + 10,
        };
        store.register_active_incident(&updated_incident).unwrap();

        let hit4 = store.check_storm_gate("deadbeefsig", 86400).unwrap().unwrap();
        assert_eq!(hit4.incident_id, "INC-MOTOCAM-102-V2");
    }
}
