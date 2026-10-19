use std::path::Path;
use std::sync::Mutex;
use chrono::Utc;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::events::{parse_stream_event, should_process_event, GerritStreamEvent};
use crate::{DaemonConfig, DaemonError};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatermarkRecord {
    pub stream_source: String,
    pub last_event_timestamp: i64,
    pub last_change_id: String,
    pub updated_at: String,
}

pub struct WatermarkStore {
    conn: Mutex<Connection>,
}

impl WatermarkStore {
    pub fn open(db_path: impl AsRef<Path>) -> Result<Self, DaemonError> {
        let path = db_path.as_ref();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }

        let conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA busy_timeout = 5000;
             CREATE TABLE IF NOT EXISTS daemon_watermarks (
                 stream_source TEXT PRIMARY KEY,
                 last_event_timestamp INTEGER NOT NULL,
                 last_change_id TEXT NOT NULL,
                 updated_at TEXT NOT NULL
             );",
        )?;

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn open_in_memory() -> Result<Self, DaemonError> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA busy_timeout = 5000;
             CREATE TABLE IF NOT EXISTS daemon_watermarks (
                 stream_source TEXT PRIMARY KEY,
                 last_event_timestamp INTEGER NOT NULL,
                 last_change_id TEXT NOT NULL,
                 updated_at TEXT NOT NULL
             );",
        )?;

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn get_watermark(&self, stream_source: &str) -> Result<Option<WatermarkRecord>, DaemonError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT stream_source, last_event_timestamp, last_change_id, updated_at
             FROM daemon_watermarks WHERE stream_source = ?1",
        )?;

        let mut rows = stmt.query(params![stream_source])?;
        if let Some(row) = rows.next()? {
            Ok(Some(WatermarkRecord {
                stream_source: row.get(0)?,
                last_event_timestamp: row.get(1)?,
                last_change_id: row.get(2)?,
                updated_at: row.get(3)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn set_watermark(
        &self,
        stream_source: &str,
        last_event_timestamp: i64,
        last_change_id: &str,
    ) -> Result<WatermarkRecord, DaemonError> {
        let updated_at = Utc::now().to_rfc3339();
        let record = WatermarkRecord {
            stream_source: stream_source.to_string(),
            last_event_timestamp,
            last_change_id: last_change_id.to_string(),
            updated_at: updated_at.clone(),
        };

        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO daemon_watermarks (stream_source, last_event_timestamp, last_change_id, updated_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(stream_source) DO UPDATE SET
                 last_event_timestamp = excluded.last_event_timestamp,
                 last_change_id = excluded.last_change_id,
                 updated_at = excluded.updated_at;",
            params![
                record.stream_source,
                record.last_event_timestamp,
                record.last_change_id,
                record.updated_at
            ],
        )?;

        Ok(record)
    }

    pub fn advance_watermark(
        &self,
        stream_source: &str,
        event_timestamp: i64,
        change_id: &str,
    ) -> Result<WatermarkRecord, DaemonError> {
        if let Some(existing) = self.get_watermark(stream_source)? {
            if existing.last_event_timestamp >= event_timestamp {
                return Ok(existing);
            }
        }

        self.set_watermark(stream_source, event_timestamp, change_id)
    }

    pub fn list_watermarks(&self) -> Result<Vec<WatermarkRecord>, DaemonError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT stream_source, last_event_timestamp, last_change_id, updated_at
             FROM daemon_watermarks ORDER BY stream_source ASC",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(WatermarkRecord {
                stream_source: row.get(0)?,
                last_event_timestamp: row.get(1)?,
                last_change_id: row.get(2)?,
                updated_at: row.get(3)?,
            })
        })?;

        let mut watermarks = Vec::new();
        for r in rows {
            watermarks.push(r?);
        }
        Ok(watermarks)
    }
}

pub fn format_catchup_query(watermark: &WatermarkRecord) -> String {
    let dt = chrono::DateTime::from_timestamp(watermark.last_event_timestamp, 0)
        .unwrap_or_else(|| chrono::DateTime::from_timestamp(0, 0).unwrap());
    let formatted = dt.format("%Y-%m-%d %H:%M:%S").to_string();
    format!("after:\"{}\"", formatted)
}

pub fn parse_catchup_output(stdout: &str, custom_prefix: Option<&str>) -> Vec<GerritStreamEvent> {
    let mut events = Vec::new();

    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if let Ok(ev) = parse_stream_event(trimmed) {
            // Gerrit query emits a final row with {"type":"stats", ...}
            if ev.event_type == "stats"
                || ev.raw_json.get("type").and_then(|t| t.as_str()) == Some("stats")
            {
                continue;
            }

            if ev.project.is_none() {
                continue;
            }

            if should_process_event(&ev, custom_prefix) {
                events.push(ev);
            }
        }
    }

    events
}

pub async fn catch_up_missed_events(
    config: &DaemonConfig,
    store: &WatermarkStore,
    stream_source: &str,
) -> Result<Vec<GerritStreamEvent>, DaemonError> {
    let watermark = match store.get_watermark(stream_source)? {
        Some(w) => w,
        None => return Ok(Vec::new()),
    };

    let query = format_catchup_query(&watermark);
    let port_str = config.gerrit_port.to_string();

    let output = tokio::process::Command::new("ssh")
        .arg("-p")
        .arg(&port_str)
        .arg(&config.gerrit_host)
        .arg("gerrit")
        .arg("query")
        .arg("--format=JSON")
        .arg("--current-patch-set")
        .arg(&query)
        .output()
        .await?;

    if !output.status.success() {
        let err_msg = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(DaemonError::Network(format!(
            "Gerrit catchup query failed (exit {}): {}",
            output.status, err_msg
        )));
    }

    let stdout_str = String::from_utf8_lossy(&output.stdout);
    let events = parse_catchup_output(&stdout_str, Some(&config.loop_drop_prefix));

    if let Some(latest) = events.iter().max_by_key(|e| e.event_created_on.unwrap_or(0)) {
        if let (Some(ts), Some(change_id)) = (latest.event_created_on, &latest.change_id) {
            store.advance_watermark(stream_source, ts, change_id)?;
        }
    }

    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_watermark_store_in_memory() {
        let store = WatermarkStore::open_in_memory().expect("open store");

        assert_eq!(store.get_watermark("gerrit.mot.com").unwrap(), None);

        let rec1 = store
            .set_watermark("gerrit.mot.com", 1700000000, "I1000")
            .unwrap();
        assert_eq!(rec1.last_event_timestamp, 1700000000);
        assert_eq!(rec1.last_change_id, "I1000");

        // Advance to older timestamp should not decrease watermark
        let rec2 = store
            .advance_watermark("gerrit.mot.com", 1699999999, "I0999")
            .unwrap();
        assert_eq!(rec2.last_event_timestamp, 1700000000);
        assert_eq!(rec2.last_change_id, "I1000");

        // Advance to newer timestamp
        let rec3 = store
            .advance_watermark("gerrit.mot.com", 1700000500, "I1005")
            .unwrap();
        assert_eq!(rec3.last_event_timestamp, 1700000500);
        assert_eq!(rec3.last_change_id, "I1005");

        let list = store.list_watermarks().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].last_event_timestamp, 1700000500);
    }

    #[test]
    fn test_format_catchup_query() {
        let wm = WatermarkRecord {
            stream_source: "test".to_string(),
            last_event_timestamp: 1700000000,
            last_change_id: "I123".to_string(),
            updated_at: "2023-11-14T22:13:20Z".to_string(),
        };

        let q = format_catchup_query(&wm);
        assert_eq!(q, "after:\"2023-11-14 22:13:20\"");
    }

    #[test]
    fn test_parse_catchup_output() {
        let raw = r#"{"project":"packages/apps/Launcher3","branch":"master","id":"I111","number":1,"currentPatchSet":{"revision":"r1"},"lastUpdated":1700000100}
{"project":"akashic/knowledge-base","branch":"master","id":"I222","number":2,"currentPatchSet":{"revision":"r2"},"lastUpdated":1700000200}
{"type":"stats","rowCount":2,"time":12}"#;

        let events = parse_catchup_output(raw, None);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].project.as_deref(), Some("packages/apps/Launcher3"));
        assert_eq!(events[0].revision.as_deref(), Some("r1"));
    }

    #[test]
    fn test_watermark_store_disk() {
        let temp = tempfile::tempdir().unwrap();
        let db_path = temp.path().join("sub/dir/rca_state.db");
        let store = WatermarkStore::open(&db_path).expect("open disk store");

        store
            .set_watermark("source-a", 1000, "Ia")
            .expect("set wm a");
        store
            .set_watermark("source-b", 2000, "Ib")
            .expect("set wm b");

        let list = store.list_watermarks().expect("list");
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].stream_source, "source-a");
        assert_eq!(list[1].stream_source, "source-b");
    }
}
