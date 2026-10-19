use rca_conductor::{
    CheckpointRecord, MockStage4Runner, Pipeline, PipelineInput, StateStore,
};
use std::path::{Path, PathBuf};

struct TempDir(PathBuf);
impl TempDir {
    fn new(prefix: &str) -> Self {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let count = COUNTER.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!(
            "{}_{}_{}_{}",
            prefix,
            std::process::id(),
            count,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&path).unwrap();
        TempDir(path)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const SAMPLE_CRASH_LOG: &str = r#"
2026-10-19 12:00:00 [DEBUG] Device IMEI: 352099001761482
2026-10-19 12:00:01 [INFO] Caller: +1-555-123-4567 email: dev@example.com
2026-10-19 12:00:02 [ERROR] java.lang.NullPointerException: Attempt to read from null object
    at com.motorola.camera3.CameraDeviceClient.notifyError(CameraDeviceClient.java:412)
    at com.motorola.camera3.feature.Mode$1.execute(Mode.kt:88)
    at android.hardware.camera2.CameraDevice.notify(CameraDevice.java:102)
Auth header: Bearer abcdef1234567890abcdef
"#;

#[tokio::test]
async fn test_full_six_stage_pipeline_e2e() {
    let vault_temp = TempDir::new("vault_e2e");
    let db_temp = TempDir::new("db_e2e");
    let db_path = db_temp.path().join("conductor.db");

    let state_store = StateStore::open(&db_path).unwrap();
    let runner = MockStage4Runner::new();
    let pipeline = Pipeline::new(state_store.clone(), runner);

    let input = PipelineInput {
        session_id: Some("sess-e2e-001".to_string()),
        jira_key: "MOTOCAM-9102".to_string(),
        app_id: "com.motorola.camera3".to_string(),
        device: "falcon".to_string(),
        android_version: "14".to_string(),
        raw_logs: SAMPLE_CRASH_LOG.to_string(),
        vault_path: vault_temp.path().to_path_buf(),
        alexandria_db: None,
        prior_resolution_id: None,
        policy_path: None,
    };

    let result = pipeline.run(input).await.expect("Pipeline run failed");

    // Assert stage completion and deduplication status
    assert!(!result.deduplicated);
    assert!(result.crash_signature.is_some());
    assert!(result.sanitized_logs.is_some());
    assert!(result.redactions_count > 0);
    assert!(result.symbolication_output.is_some());
    assert!(result.stage4_output.is_some());
    assert_eq!(result.incident_node_id.as_deref(), Some("INC-MOTOCAM-9102"));
    assert_eq!(
        result.evidence_node_id.as_deref(),
        Some("EVD-MOTOCAM-9102-STACK")
    );
    assert_eq!(
        result.resolution_node_id.as_deref(),
        Some("RES-MOTOCAM-9102-01")
    );

    // Verify session record in SQLite WAL database
    let session = state_store
        .get_session("sess-e2e-001")
        .unwrap()
        .expect("Session must exist");
    assert_eq!(session.stage, "RESOLVED");
    assert_eq!(session.status, "COMPLETED");
    assert_eq!(session.crash_signature, result.crash_signature);

    // Verify checkpoints saved in correct sequence
    let checkpoints: Vec<CheckpointRecord> =
        state_store.get_checkpoints("sess-e2e-001").unwrap();
    let stages: Vec<&str> = checkpoints.iter().map(|c| c.stage.as_str()).collect();
    assert_eq!(
        stages,
        vec![
            "TRIAGED",
            "SANITIZED",
            "SYMBOLICATED",
            "HYPOTHESIS_FALSIFIED",
            "CRYSTALLIZED",
            "RESOLVED"
        ]
    );

    // Verify Akashic vault validation returns healthy with 0 errors
    let report = rca_crystallizer::validate_vault(vault_temp.path()).unwrap();
    assert!(report.healthy, "Vault must be healthy");
    assert_eq!(report.errors, 0, "Vault must have 0 errors");
    assert_eq!(report.total_nodes, 3, "Vault must contain exactly 3 nodes");

    // Verify vault files exist on disk
    assert!(vault_temp
        .path()
        .join("incidents/INC-MOTOCAM-9102.md")
        .exists());
    assert!(vault_temp
        .path()
        .join("evidence/EVD-MOTOCAM-9102-STACK.md")
        .exists());
    assert!(vault_temp
        .path()
        .join("resolutions/RES-MOTOCAM-9102-01.md")
        .exists());
}

#[tokio::test]
async fn test_storm_gate_deduplication_early_exit() {
    let vault_temp = TempDir::new("vault_dedup");
    let state_store = StateStore::open_in_memory().unwrap();
    let pipeline = Pipeline::new(state_store.clone(), MockStage4Runner::new());

    // 1. First run creates the active incident and runs through resolution
    let input1 = PipelineInput {
        session_id: Some("sess-first".to_string()),
        jira_key: "MOTOCAM-1001".to_string(),
        app_id: "com.motorola.camera3".to_string(),
        device: "falcon".to_string(),
        android_version: "14".to_string(),
        raw_logs: SAMPLE_CRASH_LOG.to_string(),
        vault_path: vault_temp.path().to_path_buf(),
        alexandria_db: None,
        prior_resolution_id: None,
        policy_path: None,
    };

    let res1 = pipeline.run(input1).await.unwrap();
    assert!(!res1.deduplicated);

    // 2. Second run with the exact same crash within the 24h storm gate window
    let input2 = PipelineInput {
        session_id: Some("sess-duplicate".to_string()),
        jira_key: "MOTOCAM-1002".to_string(),
        app_id: "com.motorola.camera3".to_string(),
        device: "falcon".to_string(),
        android_version: "14".to_string(),
        raw_logs: SAMPLE_CRASH_LOG.to_string(),
        vault_path: vault_temp.path().to_path_buf(),
        alexandria_db: None,
        prior_resolution_id: None,
        policy_path: None,
    };

    let res2 = pipeline.run(input2).await.unwrap();

    // Verify second run was deduplicated and exited early
    assert!(res2.deduplicated);
    assert_eq!(res2.active_incident_id.as_deref(), Some("INC-MOTOCAM-1001"));
    assert_eq!(res2.crash_signature, res1.crash_signature);
    assert!(res2.stage4_output.is_none()); // Stage 4 not invoked!
    assert!(res2.resolution_node_id.is_none()); // Stage 5 & 6 not invoked!

    // Verify session record in SQLite
    let session2 = state_store.get_session("sess-duplicate").unwrap().unwrap();
    assert_eq!(session2.stage, "DEDUPLICATED");
    assert_eq!(session2.status, "DEDUPLICATED");

    // Verify checkpoints for session2 only has DEDUPLICATED
    let cps2 = state_store.get_checkpoints("sess-duplicate").unwrap();
    assert_eq!(cps2.len(), 1);
    assert_eq!(cps2[0].stage, "DEDUPLICATED");
}

#[tokio::test]
async fn test_pipeline_checkpoint_resumption() {
    let vault_temp = TempDir::new("vault_resume");
    let state_store = StateStore::open_in_memory().unwrap();
    let pipeline = Pipeline::new(state_store.clone(), MockStage4Runner::new());

    let session_id = "sess-partial-001";
    let input = PipelineInput {
        session_id: Some(session_id.to_string()),
        jira_key: "MOTOCAM-2001".to_string(),
        app_id: "com.motorola.camera3".to_string(),
        device: "hawao".to_string(),
        android_version: "15".to_string(),
        raw_logs: SAMPLE_CRASH_LOG.to_string(),
        vault_path: vault_temp.path().to_path_buf(),
        alexandria_db: None,
        prior_resolution_id: None,
        policy_path: None,
    };

    // Simulate partial execution: run stages 1, 2, and 3 only
    let mut context = rca_conductor::PipelineContext {
        session_id: session_id.to_string(),
        jira_key: input.jira_key.clone(),
        app_id: input.app_id.clone(),
        device: input.device.clone(),
        android_version: input.android_version.clone(),
        raw_logs: input.raw_logs.clone(),
        vault_path: input.vault_path.to_string_lossy().to_string(),
        ..rca_conductor::PipelineContext::default()
    };

    let now = rca_conductor::db::current_timestamp_secs();
    state_store
        .create_session(&rca_conductor::SessionRecord {
            session_id: session_id.to_string(),
            jira_key: input.jira_key.clone(),
            crash_signature: None,
            stage: "CREATED".to_string(),
            status: "IN_PROGRESS".to_string(),
            created_at: now,
            updated_at: now,
            context_json: serde_json::to_string(&context).unwrap(),
        })
        .unwrap();

    pipeline.run_stage1(&mut context).unwrap();
    pipeline.run_stage2(&mut context).unwrap();
    pipeline.run_stage3(&mut context).unwrap();

    // Verify session stopped at SYMBOLICATED
    let latest_cp = state_store
        .get_latest_checkpoint(session_id)
        .unwrap()
        .unwrap();
    assert_eq!(latest_cp.stage, "SYMBOLICATED");

    // Resume execution
    let resumed = pipeline
        .resume_session(session_id, None, None)
        .await
        .expect("Resumption must succeed");

    assert_eq!(resumed.session_id, session_id);
    assert_eq!(resumed.incident_node_id.as_deref(), Some("INC-MOTOCAM-2001"));
    assert_eq!(
        resumed.resolution_node_id.as_deref(),
        Some("RES-MOTOCAM-2001-01")
    );

    // Verify full checkpoints saved
    let checkpoints = state_store.get_checkpoints(session_id).unwrap();
    let stages: Vec<&str> = checkpoints.iter().map(|c| c.stage.as_str()).collect();
    assert_eq!(
        stages,
        vec![
            "TRIAGED",
            "SANITIZED",
            "SYMBOLICATED",
            "HYPOTHESIS_FALSIFIED",
            "CRYSTALLIZED",
            "RESOLVED"
        ]
    );

    // Verify vault validates cleanly
    let report = rca_crystallizer::validate_vault(vault_temp.path()).unwrap();
    assert!(report.healthy);
    assert_eq!(report.errors, 0);
    assert_eq!(report.total_nodes, 3);
}

#[tokio::test]
async fn test_resumption_of_completed_session() {
    let vault_temp = TempDir::new("vault_completed");
    let state_store = StateStore::open_in_memory().unwrap();
    let pipeline = Pipeline::new(state_store.clone(), MockStage4Runner::new());

    let session_id = "sess-completed";
    let input = PipelineInput {
        session_id: Some(session_id.to_string()),
        jira_key: "MOTOCAM-3001".to_string(),
        app_id: "com.motorola.camera3".to_string(),
        device: "racer".to_string(),
        android_version: "14".to_string(),
        raw_logs: SAMPLE_CRASH_LOG.to_string(),
        vault_path: vault_temp.path().to_path_buf(),
        alexandria_db: None,
        prior_resolution_id: None,
        policy_path: None,
    };

    pipeline.run(input).await.unwrap();

    let checkpoints_before = state_store.get_checkpoints(session_id).unwrap();

    // Resuming an already resolved session returns immediately without creating duplicate checkpoints
    let resumed = pipeline
        .resume_session(session_id, None, None)
        .await
        .unwrap();
    assert_eq!(resumed.session_id, session_id);

    let checkpoints_after = state_store.get_checkpoints(session_id).unwrap();
    assert_eq!(checkpoints_before.len(), checkpoints_after.len());
}
