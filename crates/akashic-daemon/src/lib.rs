pub mod a2a;
pub mod debounce;
pub mod events;
pub mod pipeline;
pub mod stream;
pub mod supervisor;
pub mod watermark;

pub use a2a::*;
pub use debounce::{CoalescedProjectBatch, DebounceQueue, PendingProjectBatch};
pub use events::{
    is_self_triggered_event, parse_stream_event, should_process_event, GerritEventType,
    GerritStreamEvent,
};
pub use pipeline::{
    discover_workspace, discover_workspace_with_vault, reconcile_project_batch, ReconcileOutcome,
    WorkspaceDiscoveryReport,
};
pub use stream::{build_ssh_stream_args, BackoffPolicy};
pub use supervisor::{DaemonStatus, StreamSupervisor};
pub use watermark::{
    catch_up_missed_events, format_catchup_query, parse_catchup_output, WatermarkRecord,
    WatermarkStore,
};

use akashic_engine::EngineConfig;
use akashic_rca::RcaConfig;
use akashic_vault::VaultConfig;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DaemonError {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Daemon network error: {0}")]
    Network(String),
    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Engine error: {0}")]
    Engine(String),
    #[error("Vault error: {0}")]
    Vault(String),
}

fn default_a2a_bind_addr() -> String {
    "127.0.0.1:9091".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    pub db_path: String,
    pub gerrit_host: String,
    pub gerrit_port: u16,
    pub debounce_quiet_secs: u64,
    pub debounce_max_cap_secs: u64,
    pub loop_drop_prefix: String,
    pub auto_discovery_enabled: bool,
    #[serde(default = "default_a2a_bind_addr")]
    pub a2a_bind_addr: String,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            db_path: ".akashic/rca_state.db".to_string(),
            gerrit_host: "gerrit.mot.com".to_string(),
            gerrit_port: 29418,
            debounce_quiet_secs: 30,
            debounce_max_cap_secs: 300,
            loop_drop_prefix: "akashic/".to_string(),
            auto_discovery_enabled: true,
            a2a_bind_addr: default_a2a_bind_addr(),
        }
    }
}

impl DaemonConfig {
    pub fn debounce_max_wait_secs(&self) -> u64 {
        self.debounce_max_cap_secs
    }
}

pub struct DaemonService {
    config: DaemonConfig,
    engine_config: EngineConfig,
    rca_config: RcaConfig,
    vault_config: VaultConfig,
}

impl DaemonService {
    pub fn new(
        config: DaemonConfig,
        engine_config: EngineConfig,
        rca_config: RcaConfig,
        vault_config: VaultConfig,
    ) -> Self {
        Self {
            config,
            engine_config,
            rca_config,
            vault_config,
        }
    }

    pub fn config(&self) -> &DaemonConfig {
        &self.config
    }

    pub fn engine_config(&self) -> &EngineConfig {
        &self.engine_config
    }

    pub fn rca_config(&self) -> &RcaConfig {
        &self.rca_config
    }

    pub fn vault_config(&self) -> &VaultConfig {
        &self.vault_config
    }

    pub fn open_watermark_store(&self) -> Result<WatermarkStore, DaemonError> {
        WatermarkStore::open(&self.config.db_path)
    }

    pub fn status(&self) -> Result<DaemonStatus, DaemonError> {
        let supervisor = StreamSupervisor::new(self.config.clone());
        supervisor.status()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_daemon_service_init() {
        let service = DaemonService::new(
            DaemonConfig::default(),
            EngineConfig::default(),
            RcaConfig::default(),
            VaultConfig::default(),
        );

        assert_eq!(service.config().gerrit_host, "gerrit.mot.com");
        assert_eq!(service.config().gerrit_port, 29418);
        assert!(service.config().auto_discovery_enabled);
        assert_eq!(service.config().debounce_max_wait_secs(), 300);
    }

    #[test]
    fn test_parse_all_gerrit_event_types() {
        // 1. change-merged
        let merged_json = r#"{
            "type": "change-merged",
            "change": {
                "project": "platform/frameworks/base",
                "branch": "main",
                "id": "Iaaa111",
                "number": 101,
                "commitMessage": "Improve system server startup"
            },
            "newRev": "rev101",
            "eventCreatedOn": 1700000100
        }"#;
        let ev = parse_stream_event(merged_json).unwrap();
        assert_eq!(ev.kind(), GerritEventType::ChangeMerged);
        assert_eq!(ev.project.as_deref(), Some("platform/frameworks/base"));
        assert_eq!(ev.revision.as_deref(), Some("rev101"));

        // 2. patchset-created
        let patchset_json = r#"{
            "type": "patchset-created",
            "change": {
                "project": "packages/apps/Dialer",
                "branch": "master",
                "id": "Ibbb222",
                "number": 102
            },
            "patchSet": {
                "revision": "rev102",
                "number": 2
            },
            "eventCreatedOn": 1700000200
        }"#;
        let ev = parse_stream_event(patchset_json).unwrap();
        assert_eq!(ev.kind(), GerritEventType::PatchsetCreated);
        assert_eq!(ev.project.as_deref(), Some("packages/apps/Dialer"));
        assert_eq!(ev.revision.as_deref(), Some("rev102"));

        // 3. change-abandoned
        let abandoned_json = r#"{
            "type": "change-abandoned",
            "change": {
                "project": "device/google/cuttlefish",
                "branch": "main",
                "id": "Iccc333",
                "number": 103
            },
            "eventCreatedOn": 1700000300
        }"#;
        let ev = parse_stream_event(abandoned_json).unwrap();
        assert_eq!(ev.kind(), GerritEventType::ChangeAbandoned);
        assert_eq!(ev.project.as_deref(), Some("device/google/cuttlefish"));

        // 4. comment-added
        let comment_json = r#"{
            "type": "comment-added",
            "change": {
                "project": "kernel/common",
                "branch": "android14-5.15",
                "id": "Iddd444",
                "number": 104
            },
            "comment": "Looks good to me, LGTM",
            "eventCreatedOn": 1700000400
        }"#;
        let ev = parse_stream_event(comment_json).unwrap();
        assert_eq!(ev.kind(), GerritEventType::CommentAdded);
        assert_eq!(ev.comment.as_deref(), Some("Looks good to me, LGTM"));

        // 5. ref-updated
        let ref_json = r#"{
            "type": "ref-updated",
            "refUpdate": {
                "project": "platform/build",
                "refName": "refs/heads/master",
                "newRev": "rev105"
            },
            "eventCreatedOn": 1700000500
        }"#;
        let ev = parse_stream_event(ref_json).unwrap();
        assert_eq!(ev.kind(), GerritEventType::RefUpdated);
        assert_eq!(ev.project.as_deref(), Some("platform/build"));
        assert_eq!(ev.branch.as_deref(), Some("refs/heads/master"));

        // 6. catchup query format (no type field, but project + id/number)
        let query_json = r#"{
            "project": "vendor/moto/camera",
            "id": "Ieee555",
            "number": 106,
            "currentPatchSet": {
                "revision": "rev106"
            },
            "lastUpdated": 1700000600
        }"#;
        let ev = parse_stream_event(query_json).unwrap();
        assert_eq!(ev.kind(), GerritEventType::CatchupChange);
        assert_eq!(ev.project.as_deref(), Some("vendor/moto/camera"));
        assert_eq!(ev.revision.as_deref(), Some("rev106"));
    }

    #[test]
    fn test_workspace_autodiscovery_and_reconciliation_e2e() {
        let temp_ws = tempdir().unwrap();
        let ws_root = temp_ws.path();

        let repo_dir = ws_root.join("packages_apps_Settings");
        fs::create_dir_all(repo_dir.join(".git")).unwrap();
        fs::create_dir_all(repo_dir.join("src/com/android/settings")).unwrap();

        // Write AndroidManifest.xml
        let manifest = r#"<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android"
    package="com.android.settings">
    <application android:label="Settings">
        <activity android:name=".SettingsActivity" />
    </application>
</manifest>"#;
        fs::write(repo_dir.join("AndroidManifest.xml"), manifest).unwrap();

        // Write Kotlin / Java source
        let source = r#"package com.android.settings;

public class SettingsActivity {
    public void onCreate() {}
}
"#;
        fs::write(
            repo_dir.join("src/com/android/settings/SettingsActivity.java"),
            source,
        )
        .unwrap();

        let store_dir = temp_ws.path().join(".akashic");
        fs::create_dir_all(&store_dir).unwrap();
        let graph_store =
            akashic_engine::store::GraphStore::open(&store_dir.join("code_graph.db")).unwrap();

        let vault_dir = temp_ws.path().join("vault");
        fs::create_dir_all(&vault_dir).unwrap();

        // Run auto discovery
        let report =
            discover_workspace_with_vault(ws_root, Some(&vault_dir), &graph_store).unwrap();
        assert_eq!(report.discovered_repositories.len(), 1);
        assert_eq!(
            report.discovered_repositories[0].repo_name,
            "packages_apps_Settings"
        );
        assert!(!report.package_mappings.is_empty());
        assert_eq!(report.provisioned_vault_partitions.len(), 1);

        // Verify partition directory structure created
        assert!(vault_dir
            .join("repositories/packages_apps_Settings/atoms")
            .exists());
        assert!(vault_dir
            .join("repositories/packages_apps_Settings/incidents")
            .exists());

        // Test WatermarkStore with reconciliation
        let wm_store =
            WatermarkStore::open(store_dir.join("rca_state.db")).unwrap();

        let batch = CoalescedProjectBatch {
            project: "packages_apps_Settings".to_string(),
            first_event_at: std::time::Instant::now(),
            last_event_at: std::time::Instant::now(),
            events: vec![],
            latest_revision: Some("HEAD".to_string()),
            latest_change_id: Some("Isettings001".to_string()),
            latest_timestamp: Some(1700002000),
        };

        let outcome = reconcile_project_batch(
            &batch,
            ws_root,
            &graph_store,
            Some(&vault_dir),
            Some(&wm_store),
            "gerrit.mot.com",
        )
        .unwrap();

        assert_eq!(outcome.project, "packages_apps_Settings");
        assert!(outcome.code_graph_reconciled);
        assert!(outcome.vault_reindexed);
        assert!(outcome.watermark_updated.is_some());
        assert_eq!(
            outcome.watermark_updated.unwrap().last_event_timestamp,
            1700002000
        );
    }
}
