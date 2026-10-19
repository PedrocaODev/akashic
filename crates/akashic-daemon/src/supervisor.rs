use std::path::Path;
use std::time::Duration;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncBufReadExt;

use akashic_engine::store::GraphStore;

use crate::debounce::DebounceQueue;
use crate::events::parse_stream_event;
use crate::pipeline::{
    discover_workspace_with_vault, reconcile_project_batch, WorkspaceDiscoveryReport,
};
use crate::stream::{build_ssh_stream_args, BackoffPolicy};
use crate::watermark::{catch_up_missed_events, WatermarkRecord, WatermarkStore};
use crate::{DaemonConfig, DaemonError};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DaemonStatus {
    pub running: bool,
    pub pid: Option<u32>,
    pub db_path: String,
    pub gerrit_host: String,
    pub gerrit_port: u16,
    pub watermarks: Vec<WatermarkRecord>,
}

pub struct StreamSupervisor {
    pub config: DaemonConfig,
    pub backoff: BackoffPolicy,
    pub debounce: DebounceQueue,
}

impl StreamSupervisor {
    pub fn new(config: DaemonConfig) -> Self {
        let debounce = DebounceQueue::from_secs(
            config.debounce_quiet_secs,
            config.debounce_max_cap_secs,
        )
        .with_loop_prefix(config.loop_drop_prefix.clone());

        Self {
            config,
            backoff: BackoffPolicy::default(),
            debounce,
        }
    }

    pub fn status(&self) -> Result<DaemonStatus, DaemonError> {
        let store = WatermarkStore::open(&self.config.db_path)?;
        let watermarks = store.list_watermarks()?;
        Ok(DaemonStatus {
            running: true,
            pid: Some(std::process::id()),
            db_path: self.config.db_path.clone(),
            gerrit_host: self.config.gerrit_host.clone(),
            gerrit_port: self.config.gerrit_port,
            watermarks,
        })
    }

    pub fn initialize_workspace(
        &self,
        workspace_root: &Path,
        code_graph_store: &GraphStore,
    ) -> Result<WorkspaceDiscoveryReport, DaemonError> {
        discover_workspace_with_vault(workspace_root, None, code_graph_store)
    }

    pub async fn run_until_shutdown(
        &mut self,
        workspace_root: &Path,
        code_graph_store: &GraphStore,
        vault_root: Option<&Path>,
        mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
    ) -> Result<(), DaemonError> {
        let watermark_store = WatermarkStore::open(&self.config.db_path)?;

        if self.config.auto_discovery_enabled {
            let _ = discover_workspace_with_vault(workspace_root, vault_root, code_graph_store)?;
        }

        let mut attempt = 0u32;

        while !*shutdown_rx.borrow() {
            // Run catch-up before streaming
            if let Ok(missed) = catch_up_missed_events(&self.config, &watermark_store, &self.config.gerrit_host).await {
                for ev in missed {
                    self.debounce.push(ev);
                }
            }

            let args = build_ssh_stream_args(&self.config);
            let mut child = match tokio::process::Command::new("ssh")
                .args(&args)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
            {
                Ok(c) => c,
                Err(_e) => {
                    let delay = self.backoff.delay_for_attempt(attempt);
                    attempt = attempt.saturating_add(1);
                    tokio::select! {
                        _ = tokio::time::sleep(delay) => {},
                        _ = shutdown_rx.changed() => break,
                    }
                    continue;
                }
            };

            let stdout = child.stdout.take();
            if let Some(stdout_stream) = stdout {
                let mut reader = tokio::io::BufReader::new(stdout_stream).lines();

                loop {
                    let sleep_duration = match self.debounce.next_deadline() {
                        Some(deadline) => {
                            let now = std::time::Instant::now();
                            if deadline <= now {
                                Duration::from_millis(0)
                            } else {
                                deadline.duration_since(now)
                            }
                        }
                        None => Duration::from_secs(3600),
                    };

                    tokio::select! {
                        _ = shutdown_rx.changed() => {
                            let _ = child.kill().await;
                            break;
                        }
                        line_res = reader.next_line() => {
                            match line_res {
                                Ok(Some(line)) => {
                                    attempt = 0;
                                    if let Ok(ev) = parse_stream_event(&line) {
                                        self.debounce.push(ev);
                                    }
                                }
                                _ => {
                                    break;
                                }
                            }
                        }
                        _ = tokio::time::sleep(sleep_duration) => {
                            let ready = self.debounce.pop_ready(std::time::Instant::now());
                            for batch in ready {
                                let _ = reconcile_project_batch(
                                    &batch,
                                    workspace_root,
                                    code_graph_store,
                                    vault_root,
                                    Some(&watermark_store),
                                    &self.config.gerrit_host,
                                );
                            }
                        }
                    }

                    if *shutdown_rx.borrow() {
                        let _ = child.kill().await;
                        break;
                    }
                }
            }

            let _ = child.wait().await;

            if *shutdown_rx.borrow() {
                break;
            }

            let delay = self.backoff.delay_for_attempt(attempt);
            attempt = attempt.saturating_add(1);
            tokio::select! {
                _ = tokio::time::sleep(delay) => {},
                _ = shutdown_rx.changed() => break,
            }
        }

        // Flush any pending debounced events
        let remaining = self.debounce.pop_ready(std::time::Instant::now() + Duration::from_secs(86400));
        for batch in remaining {
            let _ = reconcile_project_batch(
                &batch,
                workspace_root,
                code_graph_store,
                vault_root,
                Some(&watermark_store),
                &self.config.gerrit_host,
            );
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_supervisor_initialization() {
        let config = DaemonConfig::default();
        let supervisor = StreamSupervisor::new(config.clone());
        assert_eq!(supervisor.config.gerrit_port, 29418);
        assert_eq!(supervisor.config.gerrit_host, "gerrit.mot.com");
    }

    #[test]
    fn test_supervisor_status() {
        let temp = tempfile::tempdir().unwrap();
        let db_path = temp.path().join("rca_state.db");
        let mut config = DaemonConfig::default();
        config.db_path = db_path.to_string_lossy().to_string();

        let store = WatermarkStore::open(&config.db_path).unwrap();
        store.set_watermark(&config.gerrit_host, 1700000000, "Istatus").unwrap();

        let supervisor = StreamSupervisor::new(config);
        let status = supervisor.status().expect("status");
        assert!(status.running);
        assert_eq!(status.gerrit_host, "gerrit.mot.com");
        assert_eq!(status.watermarks.len(), 1);
        assert_eq!(status.watermarks[0].last_change_id, "Istatus");
    }
}
