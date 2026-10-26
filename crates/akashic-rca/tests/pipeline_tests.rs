use akashic_rca::curation::{CurationVerdict, InvestigationContextPacket};
use akashic_rca::pipeline::{ConductorPipeline, GerritEvent, PipelineInput};
use akashic_rca::stage4::{CandidateFinding, FindingOutcome, MockStage4Executor, Stage4Output};
use akashic_rca::store::{current_timestamp_secs, CheckpointRecord, IncidentClRecord, StateStore};
use akashic_rca::symbol_cache::SymbolCache;
use akashic_rca::symbolicator::{
    compute_crash_signature, parse_stack_trace, symbolicate_crash, FrameKind,
    InMemorySymbolResolver, ResolvedRepoTarget,
};
use akashic_rca::RcaError;
use akashic_vault::frontmatter::{NodeStatus, RelationType};
use akashic_vault::VaultGraph;
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

// Test 1: Complete 6-stage pipeline execution on sample crash log -> incident, evidence, resolution written, akashic-lint valid report.
#[tokio::test]
async fn test_full_six_stage_pipeline_e2e() {
    let vault_temp = TempDir::new("vault_e2e");
    let db_temp = TempDir::new("db_e2e");
    let db_path = db_temp.path().join("conductor.db");

    let state_store = StateStore::open(&db_path).unwrap();
    let runner = MockStage4Executor::new();
    let pipeline = ConductorPipeline::new(state_store.clone(), runner);

    let input = PipelineInput {
        run_id: Some("run-e2e-001".to_string()),
        legacy_session_id: None,
        jira_key: "MOTOCAM-9102".to_string(),
        app_id: "com.motorola.camera3".to_string(),
        device: "falcon".to_string(),
        android_version: "14".to_string(),
        raw_logs: SAMPLE_CRASH_LOG.to_string(),
        vault_path: vault_temp.path().to_path_buf(),
        graph_store: None,
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

    // Verify workflow run record in SQLite WAL database
    let run = state_store
        .get_workflow_run("run-e2e-001")
        .unwrap()
        .expect("Run must exist");
    assert_eq!(run.current_stage, "RESOLVED");
    assert_eq!(run.status, "COMPLETED");
    assert_eq!(run.crash_signature, result.crash_signature);

    // Verify checkpoints saved in correct sequence
    let checkpoints: Vec<CheckpointRecord> =
        state_store.get_checkpoints("run-e2e-001").unwrap();
    let stages: Vec<&str> = checkpoints.iter().map(|c| c.stage_name.as_str()).collect();
    assert_eq!(
        stages,
        vec![
            "CREATED",
            "SANITIZED",
            "SYMBOLICATED",
            "HYPOTHESIS_FALSIFIED",
            "CRYSTALLIZED",
            "RESOLVED"
        ]
    );

    // Verify Akashic vault validation returns valid report (0 errors)
    let graph = VaultGraph::from_dir(vault_temp.path()).unwrap();
    let report = akashic_lint::lint_vault(&graph);
    assert!(report.valid, "Vault must be lint-clean: {:?}", report.issues);
    assert_eq!(report.error_count, 0, "Vault must have 0 lint errors");

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

// Test 2: Checkpoint resumption: pipeline crashes at Stage 4, resumes from checkpoint to Stage 6.
#[tokio::test]
async fn test_pipeline_checkpoint_resumption() {
    let vault_temp = TempDir::new("vault_resume");
    let state_store = StateStore::open_in_memory().unwrap();
    let pipeline = ConductorPipeline::new(state_store.clone(), MockStage4Executor::new());

    let run_id = "run-partial-001";
    let input = PipelineInput {
        run_id: Some(run_id.to_string()),
        legacy_session_id: None,
        jira_key: "MOTOCAM-2001".to_string(),
        app_id: "com.motorola.camera3".to_string(),
        device: "hawao".to_string(),
        android_version: "15".to_string(),
        raw_logs: SAMPLE_CRASH_LOG.to_string(),
        vault_path: vault_temp.path().to_path_buf(),
        graph_store: None,
        prior_resolution_id: None,
        policy_path: None,
    };

    let mut context = akashic_rca::pipeline::PipelineContext {
        run_id: run_id.to_string(),
        jira_key: input.jira_key.clone(),
        app_id: input.app_id.clone(),
        device: input.device.clone(),
        android_version: input.android_version.clone(),
        raw_logs: input.raw_logs.clone(),
        vault_path: input.vault_path.to_string_lossy().to_string(),
        ..Default::default()
    };

    // Run stages 1, 2, 3 only
    pipeline.run_stage1(&mut context).unwrap();
    pipeline.run_stage2(&mut context).unwrap();
    pipeline.run_stage3(&mut context).unwrap();

    // Verify halted at SYMBOLICATED
    let latest_cp = state_store.get_latest_checkpoint(run_id).unwrap().unwrap();
    assert_eq!(latest_cp.stage_name, "SYMBOLICATED");

    // Resume execution to completion
    let resumed = pipeline
        .resume_run(run_id, None, None)
        .await
        .expect("Resumption must succeed");

    assert_eq!(resumed.run_id, run_id);
    assert_eq!(resumed.incident_node_id.as_deref(), Some("INC-MOTOCAM-2001"));
    assert_eq!(
        resumed.resolution_node_id.as_deref(),
        Some("RES-MOTOCAM-2001-01")
    );

    // Verify checkpoints include stages after resumption
    let checkpoints = state_store.get_checkpoints(run_id).unwrap();
    let stages: Vec<&str> = checkpoints.iter().map(|c| c.stage_name.as_str()).collect();
    assert_eq!(
        stages,
        vec![
            "CREATED",
            "SANITIZED",
            "SYMBOLICATED",
            "HYPOTHESIS_FALSIFIED",
            "CRYSTALLIZED",
            "RESOLVED"
        ]
    );

    // Verify vault is valid
    let graph = VaultGraph::from_dir(vault_temp.path()).unwrap();
    let report = akashic_lint::lint_vault(&graph);
    assert!(report.valid, "Resumed vault must be lint-clean");
    assert_eq!(report.error_count, 0);
}

// Test 3: Soak storm deduplication: 50 crash triggers with identical signature -> 1 incident created, hit_count = 50, supplemental evidence written, Stage 4 executed once.
#[tokio::test]
async fn test_soak_storm_deduplication_50_triggers() {
    let vault_temp = TempDir::new("vault_soak");
    let state_store = StateStore::open_in_memory().unwrap();

    // Track executions of Stage 4
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[derive(Clone)]
    struct CountingStage4Executor {
        counter: Arc<AtomicUsize>,
    }
    impl akashic_rca::stage4::Stage4Executor for CountingStage4Executor {
        async fn execute(
            &self,
            _req: &akashic_rca::stage4::Stage4Request,
        ) -> Result<Stage4Output, RcaError> {
            self.counter.fetch_add(1, Ordering::SeqCst);
            Ok(Stage4Output {
                primary_hypothesis: "Null buffer pointer in camera HAL client".to_string(),
                falsification_attempts: vec!["Checked pointer sanity".to_string()],
                invalidated_hypotheses: vec![],
                failed_attempts: vec![],
                confidence_score: 0.99,
                remediation_proposal: "Initialize buffer on open".to_string(),
            })
        }
    }

    let stage4_counter = Arc::new(AtomicUsize::new(0));
    let executor = CountingStage4Executor {
        counter: stage4_counter.clone(),
    };
    let pipeline = ConductorPipeline::new(state_store.clone(), executor);

    let mut first_sig = String::new();

    for i in 1..=50 {
        let input = PipelineInput {
            run_id: Some(format!("run-storm-{:03}", i)),
            legacy_session_id: None,
            jira_key: format!("MOTOCAM-{}", 3000 + i),
            app_id: "com.motorola.camera3".to_string(),
            device: "falcon".to_string(),
            android_version: "14".to_string(),
            raw_logs: SAMPLE_CRASH_LOG.to_string(),
            vault_path: vault_temp.path().to_path_buf(),
            graph_store: None,
            prior_resolution_id: None,
            policy_path: None,
        };

        let result = pipeline.run(input).await.expect("Run must succeed");

        if i == 1 {
            assert!(!result.deduplicated, "Run 1 must be new registered");
            first_sig = result.crash_signature.clone().unwrap();
        } else {
            assert!(result.deduplicated, "Run {} must be deduplicated", i);
            assert_eq!(
                result.incident_id.as_deref(),
                Some("INC-MOTOCAM-3001"),
                "Must link to the first incident"
            );
            assert!(result.stage4_output.is_none(), "Stage 4 must NOT run for duplicate");
        }
    }

    // Verify Stage 4 was only executed once across all 50 triggers
    assert_eq!(
        stage4_counter.load(Ordering::SeqCst),
        1,
        "Stage 4 should only execute once for the primary incident"
    );

    // Verify storm signature record in DB has hit_count == 50
    let storm_record = state_store
        .check_storm_gate(&first_sig, 86400)
        .unwrap()
        .expect("Storm signature must exist");
    assert_eq!(storm_record.hit_count, 50);
    assert_eq!(storm_record.incident_id, "INC-MOTOCAM-3001");

    // Verify primary incident exists
    assert!(vault_temp.path().join("incidents/INC-MOTOCAM-3001.md").exists());
    // Verify supplemental evidence nodes were written
    assert!(vault_temp.path().join("evidence/EVD-MOTOCAM-3050-STACK.md").exists());
}

// Test 4: Stage 5 Lint Gate failure: corrupted frontmatter or prohibited reverse relation -> pipeline rejects crystallization with VaultLintFailed.
#[tokio::test]
async fn test_stage5_lint_gate_failure() {
    let vault_temp = TempDir::new("vault_lint_fail");
    let state_store = StateStore::open_in_memory().unwrap();
    let pipeline = ConductorPipeline::new(state_store.clone(), MockStage4Executor::new());

    // Inject a malformed / invalid markdown file with prohibited reverse relation into the vault
    let bad_node_content = r#"---
id: BAD-NODE-01
title: Corrupted node with prohibited reverse relation
type: incident
status: active
valid_from: 2026-10-19
relations:
  - type: superseded_by
    target: NON_EXISTENT
---
Invalid body
"#;
    let bad_dir = vault_temp.path().join("incidents");
    std::fs::create_dir_all(&bad_dir).unwrap();
    std::fs::write(bad_dir.join("BAD-NODE-01.md"), bad_node_content).unwrap();

    let input = PipelineInput {
        run_id: Some("run-lint-fail-001".to_string()),
        legacy_session_id: None,
        jira_key: "MOTOCAM-4001".to_string(),
        app_id: "com.motorola.camera3".to_string(),
        device: "falcon".to_string(),
        android_version: "14".to_string(),
        raw_logs: SAMPLE_CRASH_LOG.to_string(),
        vault_path: vault_temp.path().to_path_buf(),
        graph_store: None,
        prior_resolution_id: None,
        policy_path: None,
    };

    let result = pipeline.run(input).await;
    match result {
        Err(RcaError::VaultLintFailed(err_count, issues)) => {
            assert!(err_count > 0, "Lint failure must have > 0 errors");
            assert!(
                issues.iter().any(|i| i.code == "L011" || i.code == "L004"),
                "Expected prohibited reverse relation error or broken link, found: {:?}",
                issues
            );
        }
        other => panic!("Expected VaultLintFailed, got: {:?}", other),
    }
}

// Test 5: Stage 6 Negative Knowledge / Gerrit abandoned event: records failed attempts in vault, forward-supersedes prior resolution.
#[tokio::test]
async fn test_stage6_negative_knowledge_and_gerrit_abandoned() {
    let vault_temp = TempDir::new("vault_gerrit");
    let state_store = StateStore::open_in_memory().unwrap();
    let pipeline = ConductorPipeline::new(state_store.clone(), MockStage4Executor::new());

    // 1. Run pipeline for original issue
    let input1 = PipelineInput {
        run_id: Some("run-gerrit-001".to_string()),
        legacy_session_id: None,
        jira_key: "MOTOCAM-5001".to_string(),
        app_id: "com.motorola.camera3".to_string(),
        device: "falcon".to_string(),
        android_version: "14".to_string(),
        raw_logs: SAMPLE_CRASH_LOG.to_string(),
        vault_path: vault_temp.path().to_path_buf(),
        graph_store: None,
        prior_resolution_id: None,
        policy_path: None,
    };

    pipeline.run(input1).await.expect("Run 1 must succeed");

    // 2. Simulate Gerrit patchset creation
    pipeline
        .handle_gerrit_event(
            &GerritEvent::PatchsetCreated {
                incident_id: "INC-MOTOCAM-5001".to_string(),
                change_id: "I1234567890abcdef".to_string(),
                cl_number: 12345,
                patchset_number: 1,
            },
            vault_temp.path(),
        )
        .unwrap();

    let cl_record = state_store
        .get_incident_cl("INC-MOTOCAM-5001")
        .unwrap()
        .expect("CL record must exist");
    assert_eq!(cl_record.status, "IN_REVIEW");
    assert_eq!(cl_record.gerrit_cl_number, 12345);

    // 3. Simulate Gerrit change abandonment
    pipeline
        .handle_gerrit_event(
            &GerritEvent::ChangeAbandoned {
                incident_id: "INC-MOTOCAM-5001".to_string(),
                change_id: "I1234567890abcdef".to_string(),
                cl_number: 12345,
                reason: "Caused regression in camera autofocus latency".to_string(),
            },
            vault_temp.path(),
        )
        .unwrap();

    let cl_record_after = state_store
        .get_incident_cl("INC-MOTOCAM-5001")
        .unwrap()
        .unwrap();
    assert_eq!(cl_record_after.status, "ABANDONED");

    // Verify negative knowledge was written to the resolution file
    let res_path = vault_temp
        .path()
        .join("resolutions/RES-MOTOCAM-5001-01.md");
    let doc = akashic_vault::parse_markdown_file(vault_temp.path(), &res_path).unwrap();
    assert_eq!(doc.frontmatter.status, NodeStatus::Abandoned);
    assert_eq!(
        doc.frontmatter.abandoned_reason.as_deref(),
        Some("Caused regression in camera autofocus latency")
    );
    assert!(doc
        .frontmatter
        .failed_attempts
        .iter()
        .any(|a| a.contains("12345") && a.contains("abandoned")));

    // 4. Create second resolution superseding the abandoned resolution
    let input2 = PipelineInput {
        run_id: Some("run-gerrit-002".to_string()),
        legacy_session_id: None,
        jira_key: "MOTOCAM-5002".to_string(),
        app_id: "com.motorola.camera3".to_string(),
        device: "falcon".to_string(),
        android_version: "14".to_string(),
        raw_logs: SAMPLE_CRASH_LOG.to_string(),
        vault_path: vault_temp.path().to_path_buf(),
        graph_store: None,
        prior_resolution_id: Some("RES-MOTOCAM-5001-01".to_string()),
        policy_path: None,
    };

    // Before running input2, clear storm gate so it processes as a new incident or alternative fix
    let state_store2 = StateStore::open_in_memory().unwrap();
    let pipeline2 = ConductorPipeline::new(state_store2, MockStage4Executor::new());

    let res2 = pipeline2.run(input2).await.expect("Run 2 must succeed");
    assert_eq!(res2.resolution_node_id.as_deref(), Some("RES-MOTOCAM-5002-01"));

    // Verify prior resolution on disk retains NodeStatus::Abandoned (ADR 0003 forward-only supersession)
    let doc1_after = akashic_vault::parse_markdown_file(vault_temp.path(), &res_path).unwrap();
    assert_eq!(
        doc1_after.frontmatter.status,
        NodeStatus::Abandoned,
        "Prior resolution must retain Abandoned status on disk"
    );

    let res2_path = vault_temp
        .path()
        .join("resolutions/RES-MOTOCAM-5002-01.md");
    let doc2 = akashic_vault::parse_markdown_file(vault_temp.path(), &res2_path).unwrap();
    assert!(
        matches!(doc2.frontmatter.status, NodeStatus::Active | NodeStatus::Proposed),
        "New resolution must have Active or Proposed status, got {:?}",
        doc2.frontmatter.status
    );
    assert!(
        doc2.frontmatter
            .relations
            .iter()
            .any(|r| r.relation_type == RelationType::Supersedes
                && r.target == "RES-MOTOCAM-5001-01"),
        "Must record forward supersession to prior resolution"
    );
}

// Test 6: SymbolCache LRU eviction and symbolication fidelity.
#[test]
fn test_symbol_cache_lru_and_symbolication_fidelity() {
    let cache_dir = TempDir::new("symbol_cache");
    // Limit cache to 1 MB for testing LRU eviction
    let cache = SymbolCache::new(cache_dir.path(), 1).expect("Cache init failed");

    // Put 3 artifacts of ~500 KB each (total 1.5 MB -> triggers LRU eviction of oldest)
    let artifact_1 = vec![0x41u8; 500 * 1024];
    let artifact_2 = vec![0x42u8; 500 * 1024];
    let artifact_3 = vec![0x43u8; 500 * 1024];

    cache.put_artifact("build_1", &artifact_1).unwrap();
    assert!(cache.get_artifact("build_1").is_some());

    cache.put_artifact("build_2", &artifact_2).unwrap();
    assert!(cache.get_artifact("build_2").is_some());

    cache.put_artifact("build_3", &artifact_3).unwrap();
    assert!(cache.get_artifact("build_3").is_some());

    // Total size should not exceed 1 MB (1024 * 1024 bytes)
    assert!(
        cache.total_size_bytes() <= 1024 * 1024,
        "Total cache size {} must not exceed 1 MB",
        cache.total_size_bytes()
    );

    // Oldest artifact build_1 should have been evicted
    assert!(
        cache.get_artifact("build_1").is_none(),
        "build_1 should have been evicted by LRU"
    );

    // Verify symbolication fidelity
    let mut resolver = InMemorySymbolResolver::new();
    resolver.add_mapping(
        "com.motorola.camera3.CameraDeviceClient.notifyError",
        Some("com.motorola.camera3"),
        ResolvedRepoTarget {
            repo_name: "vendor/motorola/apps/Camera3".to_string(),
            package_id: "com.motorola.camera3".to_string(),
            file: Some("CameraDeviceClient.java".to_string()),
            line: Some(412),
            symbol: "com.motorola.camera3.CameraDeviceClient.notifyError".to_string(),
            class_name: Some("com.motorola.camera3.CameraDeviceClient".to_string()),
        },
    );

    let (exc, frames) = parse_stack_trace(SAMPLE_CRASH_LOG);
    assert_eq!(exc, "java.lang.NullPointerException");
    assert_eq!(frames.len(), 3);
    assert_eq!(frames[0].frame_kind, FrameKind::JavaKotlin);

    let sig = compute_crash_signature(
        "com.motorola.camera3",
        &exc,
        &frames
            .iter()
            .map(|f| f.raw_symbol.clone().unwrap_or_default())
            .collect::<Vec<_>>(),
    );
    assert_eq!(sig.len(), 64);
    assert!(sig.chars().all(|c| c.is_ascii_hexdigit()));

    let sym_result = symbolicate_crash("com.motorola.camera3", SAMPLE_CRASH_LOG, &resolver).unwrap();
    let fault = sym_result.faulting_symbol.as_ref().expect("faulting symbol should be resolved");
    assert_eq!(fault.repo_name, "vendor/motorola/apps/Camera3");
    assert_eq!(fault.package_id, "com.motorola.camera3");
}

// Test 7 (Seam 1): Two-phase prepare and submit finding flow
#[tokio::test]
async fn test_two_phase_prepare_and_submit_finding() {
    let vault_temp = TempDir::new("vault_twophase");
    let db_temp = TempDir::new("db_twophase");
    let db_path = db_temp.path().join("conductor.db");

    let state_store = StateStore::open(&db_path).unwrap();
    let runner = MockStage4Executor::new();
    let pipeline = ConductorPipeline::new(state_store.clone(), runner);

    let input = PipelineInput {
        run_id: Some("run-twophase-001".to_string()),
        legacy_session_id: None,
        jira_key: "MOTOCAM-9201".to_string(),
        app_id: "com.motorola.camera3".to_string(),
        device: "falcon".to_string(),
        android_version: "14".to_string(),
        raw_logs: SAMPLE_CRASH_LOG.to_string(),
        vault_path: vault_temp.path().to_path_buf(),
        graph_store: None,
        prior_resolution_id: None,
        policy_path: None,
    };

    // Phase A: prepare_investigation
    let (context, packet_opt) = pipeline
        .prepare_investigation(input)
        .expect("prepare_investigation should succeed");
    assert_eq!(context.run_id, "run-twophase-001");
    let packet = packet_opt.expect("InvestigationContextPacket should be assembled");
    assert_eq!(packet.jira_key, "MOTOCAM-9201");
    assert_eq!(packet.app_id, "com.motorola.camera3");
    assert!(!packet.crash_signature.is_empty());
    assert!(packet.redactions_count > 0);

    // Verify workflow run is checkpointed at AWAITING_REASONING
    let run_rec = state_store
        .get_workflow_run("run-twophase-001")
        .unwrap()
        .expect("Workflow run record must exist");
    assert_eq!(run_rec.status, "AWAITING_REASONING");
    assert_eq!(run_rec.current_stage, "AWAITING_REASONING");

    // Phase B: submit valid CandidateFinding
    let faulting_sym = packet
        .symbolication
        .faulting_symbol
        .as_ref()
        .map(|f| f.symbol.clone())
        .unwrap_or_else(|| "com.motorola.camera3.CameraDeviceClient.notifyError".to_string());

    let finding = CandidateFinding {
        candidate_id: Some("cand-001".to_string()),
        agent_id: Some("gemini-specialist-1".to_string()),
        run_id: Some("run-twophase-001".to_string()),
        storm_candidate_id: None,
        outcome: FindingOutcome::Resolved,
        primary_hypothesis: format!("Null pointer dereference in {}", faulting_sym),
        falsification_attempts: vec!["Checked CameraDeviceClient null safety check".to_string()],
        invalidated_hypotheses: vec!["IPC binder disconnect hypothesis disproven".to_string()],
        failed_attempts: vec![],
        confidence_score: 0.95,
        remediation_proposal: format!("Add null check prior to calling {}", faulting_sym),
        remediation_type: Some("permanent_fix".to_string()),
        hold_for_curation: false,
        affected_symbols: vec![faulting_sym],
        supersedes_resolution_id: None,
    };

    let (_ctx_out, report) = pipeline
        .submit_finding("run-twophase-001", &finding, vault_temp.path())
        .expect("submit_finding should succeed");

    assert_eq!(report.verdict, CurationVerdict::Approved);
    assert_eq!(
        report.incident_node_id.as_deref(),
        Some("INC-MOTOCAM-9201")
    );
    assert_eq!(
        report.evidence_node_id.as_deref(),
        Some("EVD-MOTOCAM-9201-STACK")
    );
    assert_eq!(
        report.resolution_node_id.as_deref(),
        Some("RES-MOTOCAM-9201-01")
    );

    // Assert files written to disk
    assert!(vault_temp.path().join("incidents/INC-MOTOCAM-9201.md").exists());
    assert!(vault_temp.path().join("evidence/EVD-MOTOCAM-9201-STACK.md").exists());
    assert!(vault_temp.path().join("resolutions/RES-MOTOCAM-9201-01.md").exists());

    // Lint vault: 0 errors
    let graph = VaultGraph::from_dir(vault_temp.path()).expect("VaultGraph loading must succeed");
    let lint_report = akashic_lint::lint_vault(&graph);
    let error_count = lint_report
        .issues
        .iter()
        .filter(|i| i.severity == akashic_lint::Severity::Error)
        .count();
    assert_eq!(error_count, 0, "Vault must have 0 lint errors");

    // Verify HYPOTHESIS_FALSIFIED checkpoint and curation report persistence
    let checkpoints = state_store.get_checkpoints("run-twophase-001").unwrap();
    assert!(
        checkpoints.iter().any(|c| c.stage_name == "HYPOTHESIS_FALSIFIED"),
        "Checkpoints must include HYPOTHESIS_FALSIFIED"
    );
    let cur_report = state_store.get_curation_report("run-twophase-001").unwrap();
    assert!(
        cur_report.is_some(),
        "Curation report must be persisted on workflow run"
    );
}

// Test 8 (Seam 1): Curation gate invariants C001, C002, and C004 enforcement
#[tokio::test]
async fn test_curation_gate_invariants_c001_c002_c004() {
    let vault_temp = TempDir::new("vault_invariants");
    let db_temp = TempDir::new("db_invariants");
    let db_path = db_temp.path().join("conductor.db");

    let state_store = StateStore::open(&db_path).unwrap();
    let runner = MockStage4Executor::new();
    let pipeline = ConductorPipeline::new(state_store.clone(), runner);

    let input = PipelineInput {
        run_id: Some("run-inv-001".to_string()),
        legacy_session_id: None,
        jira_key: "MOTOCAM-9202".to_string(),
        app_id: "com.motorola.camera3".to_string(),
        device: "falcon".to_string(),
        android_version: "14".to_string(),
        raw_logs: SAMPLE_CRASH_LOG.to_string(),
        vault_path: vault_temp.path().to_path_buf(),
        graph_store: None,
        prior_resolution_id: None,
        policy_path: None,
    };

    let (_context, packet_opt) = pipeline
        .prepare_investigation(input)
        .expect("prepare_investigation should succeed");
    let mut packet = packet_opt.unwrap();

    // Populate Negative Knowledge
    packet.historical_invalidated_hypotheses.push("IPC binder disconnection on camera start".to_string());
    packet.historical_failed_attempts.push("Increase timeout to 5000ms".to_string());
    state_store
        .save_context_packet("run-inv-001", &serde_json::to_string(&packet).unwrap())
        .unwrap();

    // Invariant C001: Repeating invalidated hypothesis (case-insensitive check)
    let finding_c001_hyp = CandidateFinding {
        candidate_id: Some("cand-c001-a".to_string()),
        agent_id: Some("agent-1".to_string()),
        run_id: Some("run-inv-001".to_string()),
        storm_candidate_id: None,
        outcome: FindingOutcome::Resolved,
        primary_hypothesis: "ipc binder disconnection on camera start".to_string(),
        falsification_attempts: vec!["Checked binder stats".to_string()],
        invalidated_hypotheses: vec![],
        failed_attempts: vec![],
        confidence_score: 0.8,
        remediation_proposal: "Restart binder service".to_string(),
        remediation_type: Some("permanent_fix".to_string()),
        hold_for_curation: false,
        affected_symbols: vec![],
        supersedes_resolution_id: None,
    };

    let (_ctx_out, rep_c001_a) = pipeline
        .submit_finding("run-inv-001", &finding_c001_hyp, vault_temp.path())
        .unwrap();
    assert_eq!(rep_c001_a.verdict, CurationVerdict::Rejected);
    assert!(rep_c001_a.issues.iter().any(|i| i.invariant == "C001"));
    assert!(!vault_temp.path().join("incidents/INC-MOTOCAM-9202.md").exists());

    // Invariant C001: Repeating failed remediation proposal (case-insensitive check)
    let finding_c001_rem = CandidateFinding {
        candidate_id: Some("cand-c001-b".to_string()),
        agent_id: Some("agent-1".to_string()),
        run_id: Some("run-inv-001".to_string()),
        storm_candidate_id: None,
        outcome: FindingOutcome::Resolved,
        primary_hypothesis: "Thread pool starvation in HAL client".to_string(),
        falsification_attempts: vec!["Thread count analysis".to_string()],
        invalidated_hypotheses: vec![],
        failed_attempts: vec![],
        confidence_score: 0.8,
        remediation_proposal: "increase timeout to 5000ms".to_string(),
        remediation_type: Some("permanent_fix".to_string()),
        hold_for_curation: false,
        affected_symbols: vec![],
        supersedes_resolution_id: None,
    };

    let (_ctx_out, rep_c001_b) = pipeline
        .submit_finding("run-inv-001", &finding_c001_rem, vault_temp.path())
        .unwrap();
    assert_eq!(rep_c001_b.verdict, CurationVerdict::Rejected);
    assert!(rep_c001_b.issues.iter().any(|i| i.invariant == "C001"));

    // Invariant C002: Missing falsification trace for Resolved finding
    let finding_c002_resolved = CandidateFinding {
        candidate_id: Some("cand-c002-a".to_string()),
        agent_id: Some("agent-1".to_string()),
        run_id: Some("run-inv-001".to_string()),
        storm_candidate_id: None,
        outcome: FindingOutcome::Resolved,
        primary_hypothesis: "Null pointer in CameraDeviceClient".to_string(),
        falsification_attempts: vec![], // Empty falsification attempts
        invalidated_hypotheses: vec![],
        failed_attempts: vec![],
        confidence_score: 0.8,
        remediation_proposal: "Add guard check".to_string(),
        remediation_type: Some("permanent_fix".to_string()),
        hold_for_curation: false,
        affected_symbols: vec![],
        supersedes_resolution_id: None,
    };

    let (_ctx_out, rep_c002) = pipeline
        .submit_finding("run-inv-001", &finding_c002_resolved, vault_temp.path())
        .unwrap();
    assert_eq!(rep_c002.verdict, CurationVerdict::Rejected);
    assert!(rep_c002.issues.iter().any(|i| i.invariant == "C002"));

    // Invariant C004: In-memory MigrationPlan preview lint violation check
    // Write an invalid file to vault that fails vault linting (broken relation target L004 error)
    let inc_dir = vault_temp.path().join("incidents");
    std::fs::create_dir_all(&inc_dir).unwrap();
    std::fs::write(
        inc_dir.join("BROKEN.md"),
        r#"---
id: BROKEN
title: Broken Incident Node
type: incident
status: active
scope:
  app: com.motorola.camera3
valid_from: 2026-10-19
relations:
  - target: NON_EXISTENT_NODE_12345
    type: supported_by
---
Broken relation target causes L004 error.
"#,
    ).unwrap();

    let valid_finding = CandidateFinding {
        candidate_id: Some("cand-c004".to_string()),
        agent_id: Some("agent-1".to_string()),
        run_id: Some("run-inv-001".to_string()),
        storm_candidate_id: None,
        outcome: FindingOutcome::Resolved,
        primary_hypothesis: "Valid hypothesis for CameraDeviceClient".to_string(),
        falsification_attempts: vec!["Valid falsification trace".to_string()],
        invalidated_hypotheses: vec![],
        failed_attempts: vec![],
        confidence_score: 0.9,
        remediation_proposal: "Valid remediation proposal".to_string(),
        remediation_type: Some("permanent_fix".to_string()),
        hold_for_curation: false,
        affected_symbols: vec!["com.motorola.camera3.CameraDeviceClient.notifyError".to_string()],
        supersedes_resolution_id: None,
    };

    let (_ctx_out, rep_c004) = pipeline
        .submit_finding("run-inv-001", &valid_finding, vault_temp.path())
        .unwrap();
    assert_eq!(rep_c004.verdict, CurationVerdict::Rejected);
    assert!(rep_c004.issues.iter().any(|i| i.invariant == "C004"));
    // Assert zero new incident files for MOTOCAM-9202 written to disk
    assert!(!vault_temp.path().join("incidents/INC-MOTOCAM-9202.md").exists());
}

// Test 9 (Seam 1): Partial crystallization on inconclusive finding
#[tokio::test]
async fn test_partial_crystallization_inconclusive() {
    let vault_temp = TempDir::new("vault_inconclusive");
    let db_temp = TempDir::new("db_inconclusive");
    let db_path = db_temp.path().join("conductor.db");

    let state_store = StateStore::open(&db_path).unwrap();
    let runner = MockStage4Executor::new();
    let pipeline = ConductorPipeline::new(state_store.clone(), runner);

    let input = PipelineInput {
        run_id: Some("run-inconclusive-001".to_string()),
        legacy_session_id: None,
        jira_key: "MOTOCAM-9301".to_string(),
        app_id: "com.motorola.camera3".to_string(),
        device: "falcon".to_string(),
        android_version: "14".to_string(),
        raw_logs: SAMPLE_CRASH_LOG.to_string(),
        vault_path: vault_temp.path().to_path_buf(),
        graph_store: None,
        prior_resolution_id: None,
        policy_path: None,
    };

    let (_context, _packet) = pipeline
        .prepare_investigation(input)
        .expect("prepare_investigation should succeed");

    let finding = CandidateFinding {
        candidate_id: Some("cand-inc-001".to_string()),
        agent_id: Some("agent-investigator".to_string()),
        run_id: Some("run-inconclusive-001".to_string()),
        storm_candidate_id: None,
        outcome: FindingOutcome::Inconclusive,
        primary_hypothesis: "Investigation yielded inconclusive root cause".to_string(),
        falsification_attempts: vec!["Checked memory heap dump".to_string()],
        invalidated_hypotheses: vec!["Suspected out of memory error in ImageReader buffer".to_string()],
        failed_attempts: vec!["Increased buffer size to 64MB".to_string()],
        confidence_score: 0.3,
        remediation_proposal: "".to_string(),
        remediation_type: None,
        hold_for_curation: false,
        affected_symbols: vec![],
        supersedes_resolution_id: None,
    };

    let (_ctx_out, report) = pipeline
        .submit_finding("run-inconclusive-001", &finding, vault_temp.path())
        .expect("submit_finding should succeed");

    assert_eq!(report.verdict, CurationVerdict::Approved);
    assert_eq!(
        report.incident_node_id.as_deref(),
        Some("INC-MOTOCAM-9301")
    );
    assert_eq!(
        report.evidence_node_id.as_deref(),
        Some("EVD-MOTOCAM-9301-STACK")
    );
    assert!(report.resolution_node_id.is_none());

    // Verify INC-* and EVD-* exist on disk, but NO RES-* exists
    assert!(vault_temp.path().join("incidents/INC-MOTOCAM-9301.md").exists());
    assert!(vault_temp.path().join("evidence/EVD-MOTOCAM-9301-STACK.md").exists());
    assert!(!vault_temp.path().join("resolutions/RES-MOTOCAM-9301-01.md").exists());

    // Verify negative knowledge was recorded in the evidence node
    let evd_doc = akashic_vault::parse_markdown_file(
        vault_temp.path(),
        &vault_temp.path().join("evidence/EVD-MOTOCAM-9301-STACK.md"),
    )
    .unwrap();
    assert!(evd_doc
        .frontmatter
        .invalidated_hypotheses
        .iter()
        .any(|h| h.contains("Suspected out of memory error")));

    // Verify run status in state store
    let run_rec = state_store
        .get_workflow_run("run-inconclusive-001")
        .unwrap()
        .expect("Workflow run record must exist");
    assert_eq!(run_rec.status, "INCONCLUSIVE");
    assert_eq!(run_rec.current_stage, "INCONCLUSIVE");
}

// Test 10 (Seam 1): Multi-agent competing findings and adjudication
#[tokio::test]
async fn test_multi_agent_competing_findings_and_adjudication() {
    let vault_temp = TempDir::new("vault_compete");
    let db_temp = TempDir::new("db_compete");
    let db_path = db_temp.path().join("conductor.db");

    let state_store = StateStore::open(&db_path).unwrap();
    let runner = MockStage4Executor::new();
    let pipeline = ConductorPipeline::new(state_store.clone(), runner);

    let input = PipelineInput {
        run_id: Some("run-compete-001".to_string()),
        legacy_session_id: None,
        jira_key: "MOTOCAM-9401".to_string(),
        app_id: "com.motorola.camera3".to_string(),
        device: "falcon".to_string(),
        android_version: "14".to_string(),
        raw_logs: SAMPLE_CRASH_LOG.to_string(),
        vault_path: vault_temp.path().to_path_buf(),
        graph_store: None,
        prior_resolution_id: None,
        policy_path: None,
    };

    let (_context, _packet) = pipeline
        .prepare_investigation(input)
        .expect("prepare_investigation should succeed");

    // Agent Alpha submits finding with hold_for_curation = true
    let cand_alpha = CandidateFinding {
        candidate_id: Some("cand-alpha".to_string()),
        agent_id: Some("agent-alpha".to_string()),
        run_id: Some("run-compete-001".to_string()),
        storm_candidate_id: None,
        outcome: FindingOutcome::Resolved,
        primary_hypothesis: "Race condition in CameraDeviceClient.notifyError".to_string(),
        falsification_attempts: vec!["Inspected thread synchronization locks".to_string()],
        invalidated_hypotheses: vec![],
        failed_attempts: vec![],
        confidence_score: 0.92,
        remediation_proposal: "Synchronize notifyError on client mutex".to_string(),
        remediation_type: Some("permanent_fix".to_string()),
        hold_for_curation: true,
        affected_symbols: vec!["com.motorola.camera3.CameraDeviceClient.notifyError".to_string()],
        supersedes_resolution_id: None,
    };

    let (_ctx_a, rep_alpha) = pipeline
        .submit_finding("run-compete-001", &cand_alpha, vault_temp.path())
        .expect("Submit candidate alpha should succeed");
    assert_eq!(rep_alpha.verdict, CurationVerdict::HeldForCuration);

    // Agent Beta submits competing finding
    let cand_beta = CandidateFinding {
        candidate_id: Some("cand-beta".to_string()),
        agent_id: Some("agent-beta".to_string()),
        run_id: Some("run-compete-001".to_string()),
        storm_candidate_id: None,
        outcome: FindingOutcome::Resolved,
        primary_hypothesis: "Buffer allocation exhausted during frame processing".to_string(),
        falsification_attempts: vec!["Analyzed buffer allocation pools in CameraDeviceClient".to_string()],
        invalidated_hypotheses: vec![],
        failed_attempts: vec![],
        confidence_score: 0.88,
        remediation_proposal: "Increase buffer allocation limit in CameraDeviceClient".to_string(),
        remediation_type: Some("workaround".to_string()),
        hold_for_curation: false,
        affected_symbols: vec!["com.motorola.camera3.CameraDeviceClient.notifyError".to_string()],
        supersedes_resolution_id: None,
    };

    let (_ctx_b, rep_beta) = pipeline
        .submit_finding("run-compete-001", &cand_beta, vault_temp.path())
        .expect("Submit candidate beta should succeed");
    assert_eq!(rep_beta.verdict, CurationVerdict::HeldForCuration);

    // Verify workflow run is in AWAITING_CURATION and has 2 candidate findings
    let run_rec = state_store
        .get_workflow_run("run-compete-001")
        .unwrap()
        .expect("Workflow run record must exist");
    assert_eq!(run_rec.status, "AWAITING_CURATION");
    assert_eq!(run_rec.current_stage, "AWAITING_CURATION");

    let stored_candidates = state_store.list_candidate_findings("run-compete-001").unwrap();
    assert_eq!(stored_candidates.len(), 2);

    // Adjudicate: select Agent Alpha as the winner
    let (_ctx_final, rep_final) = pipeline
        .curate_investigation("run-compete-001", "cand-alpha", vault_temp.path())
        .expect("curate_investigation should succeed");
    assert_eq!(rep_final.verdict, CurationVerdict::Approved);

    // Verify candidate finding statuses in store
    let updated_candidates = state_store.list_candidate_findings("run-compete-001").unwrap();
    let alpha_rec = updated_candidates.iter().find(|c| c.candidate_id == "cand-alpha").unwrap();
    let beta_rec = updated_candidates.iter().find(|c| c.candidate_id == "cand-beta").unwrap();
    assert_eq!(alpha_rec.status, "ACCEPTED");
    assert_eq!(beta_rec.status, "REJECTED");

    // Verify that candidate beta's hypothesis was folded into the crystallized resolution's invalidated_hypotheses
    let res_path = vault_temp.path().join("resolutions/RES-MOTOCAM-9401-01.md");
    let doc = akashic_vault::parse_markdown_file(vault_temp.path(), &res_path).unwrap();
    assert!(
        doc.frontmatter
            .invalidated_hypotheses
            .iter()
            .any(|h| h.contains("Buffer allocation exhausted")),
        "Losing hypothesis must be folded into winning resolution's invalidated_hypotheses"
    );
}

// Test 11 (Seam 1): Gerrit change-abandoned auto-requeue and storm signature relinking
#[tokio::test]
async fn test_gerrit_change_abandoned_auto_requeue() {
    let vault_temp = TempDir::new("vault_requeue");
    let db_temp = TempDir::new("db_requeue");
    let db_path = db_temp.path().join("conductor.db");

    let state_store = StateStore::open(&db_path).unwrap();
    let runner = MockStage4Executor::new();
    let pipeline = ConductorPipeline::new(state_store.clone(), runner);

    let input = PipelineInput {
        run_id: Some("run-requeue-001".to_string()),
        legacy_session_id: None,
        jira_key: "MOTOCAM-9501".to_string(),
        app_id: "com.motorola.camera3".to_string(),
        device: "falcon".to_string(),
        android_version: "14".to_string(),
        raw_logs: SAMPLE_CRASH_LOG.to_string(),
        vault_path: vault_temp.path().to_path_buf(),
        graph_store: None,
        prior_resolution_id: None,
        policy_path: None,
    };

    let result = pipeline.run(input).await.expect("Initial run must succeed");
    let orig_crash_sig = result.crash_signature.expect("Crash signature must exist");

    // Associate CL with incident
    let cl_rec = IncidentClRecord {
        incident_id: "INC-MOTOCAM-9501".to_string(),
        resolution_id: "RES-MOTOCAM-9501-01".to_string(),
        repo_name: "vendor/motorola/apps/Camera3".to_string(),
        gerrit_change_id: "Ideadbeef99887766".to_string(),
        gerrit_cl_number: 998877,
        patchset_number: 1,
        status: "IN_REVIEW".to_string(),
        updated_at: current_timestamp_secs(),
    };
    state_store.upsert_incident_cl(&cl_rec).unwrap();

    // Trigger Gerrit ChangeAbandoned event
    pipeline
        .handle_gerrit_event(
            &GerritEvent::ChangeAbandoned {
                incident_id: "INC-MOTOCAM-9501".to_string(),
                change_id: "Ideadbeef99887766".to_string(),
                cl_number: 998877,
                reason: "Patchset introduces deadlock in camera capture service".to_string(),
            },
            vault_temp.path(),
        )
        .expect("handle_gerrit_event ChangeAbandoned must succeed");

    // 1. Verify incident_cls status is ABANDONED
    let cl_after = state_store
        .get_incident_cl("INC-MOTOCAM-9501")
        .unwrap()
        .expect("CL record must exist");
    assert_eq!(cl_after.status, "ABANDONED");

    // 2. Verify RES-* node in vault has status: abandoned and records failed attempt
    let res_path = vault_temp.path().join("resolutions/RES-MOTOCAM-9501-01.md");
    let doc = akashic_vault::parse_markdown_file(vault_temp.path(), &res_path).unwrap();
    assert_eq!(doc.frontmatter.status, NodeStatus::Abandoned);
    assert_eq!(
        doc.frontmatter.abandoned_reason.as_deref(),
        Some("Patchset introduces deadlock in camera capture service")
    );
    assert!(doc
        .frontmatter
        .failed_attempts
        .iter()
        .any(|a| a.contains("998877") && a.contains("abandoned")));

    // 3. Verify follow-up workflow run auto-requeued in AWAITING_REASONING
    let retry_run_id = "run-requeue-001-retry-998877";
    let retry_run = state_store
        .get_workflow_run(retry_run_id)
        .unwrap()
        .expect("Retry workflow run must exist");
    assert_eq!(retry_run.status, "AWAITING_REASONING");
    assert_eq!(retry_run.current_stage, "AWAITING_REASONING");

    // 4. Verify refreshed context packet has historical failed attempts and prior resolution
    let retry_packet_json = state_store
        .get_context_packet(retry_run_id)
        .unwrap()
        .expect("Context packet must exist for retry run");
    let retry_packet: InvestigationContextPacket = serde_json::from_str(&retry_packet_json).unwrap();
    assert!(retry_packet
        .historical_failed_attempts
        .iter()
        .any(|a| a.contains("998877") && a.contains("abandoned")));
    assert!(retry_packet
        .prior_resolutions
        .iter()
        .any(|p| p.resolution_id == "RES-MOTOCAM-9501-01" && p.status == "abandoned"));

    // 5. Verify storm signature relinking
    let storm_rec = state_store
        .check_storm_gate(&orig_crash_sig, 86400)
        .unwrap()
        .expect("Storm signature record must exist");
    assert_eq!(storm_rec.active_run_id, retry_run_id);
}
