use akashic_daemon::a2a::{A2aAgentCard, A2aServer, A2aServerConfig, A2aTask, A2aTaskState};
use akashic_rca::{CandidateFinding, FindingOutcome};
use std::fs;
use std::sync::Arc;
use tempfile::tempdir;

#[tokio::test]
async fn test_a2a_agent_card_and_jsonrpc_lifecycle() {
    let temp = tempdir().unwrap();
    let db_path = temp.path().join("rca_state.db");
    let vault_path = temp.path().join("vault");
    fs::create_dir_all(vault_path.join("incidents")).unwrap();
    fs::create_dir_all(vault_path.join("evidence")).unwrap();
    fs::create_dir_all(vault_path.join("resolutions")).unwrap();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let local_addr = listener.local_addr().unwrap();

    let config = A2aServerConfig {
        bind_addr: local_addr.to_string(),
        db_path,
        vault_path: vault_path.clone(),
        graph_store: None,
        policy_path: None,
    };

    let server = Arc::new(A2aServer::new(config).unwrap());
    let server_clone = Arc::clone(&server);

    tokio::spawn(async move {
        server_clone.serve(listener).await.unwrap();
    });

    let client = reqwest::Client::new();
    let base_url = format!("http://{}", local_addr);

    // 1. GET /.well-known/agent.json -> validate A2aAgentCard schema
    let card_resp = client
        .get(format!("{}/.well-known/agent.json", base_url))
        .send()
        .await
        .unwrap();

    assert_eq!(card_resp.status(), 200);
    let card: A2aAgentCard = card_resp.json().await.unwrap();
    assert_eq!(card.name, "akashic-rca");
    assert_eq!(card.version, "1.0.0");
    assert_eq!(card.protocol_version, "1.0");
    assert!(card.skills.contains(&"prepare_investigation".to_string()));
    assert!(card.skills.contains(&"submit_finding".to_string()));
    assert!(card.skills.contains(&"curate_investigation".to_string()));

    // 2. POST /a2a: tasks/send (Phase A trigger)
    let logs = r#"
FATAL EXCEPTION: main
Process: com.motorola.camera, PID: 12345
java.lang.NullPointerException: Attempt to invoke virtual method on a null object reference
    at com.motorola.camera.CameraApp.init(CameraApp.java:42)
"#;

    let trigger_rpc = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tasks/send",
        "params": {
            "jira_key": "INC-A2A-01",
            "app_id": "com.motorola.camera",
            "device": "eqs",
            "android_version": "14",
            "raw_logs": logs
        }
    });

    let trigger_resp = client
        .post(format!("{}/a2a", base_url))
        .json(&trigger_rpc)
        .send()
        .await
        .unwrap();

    assert_eq!(trigger_resp.status(), 200);
    let trigger_val: serde_json::Value = trigger_resp.json().await.unwrap();
    assert_eq!(trigger_val["id"], 1);
    let task: A2aTask = serde_json::from_value(trigger_val["result"].clone()).unwrap();
    assert_eq!(task.state, A2aTaskState::InputRequired);
    assert!(task.context_packet.is_some());
    let run_id = task.id.clone();

    // 3. POST /a2a: tasks/get
    let get_rpc = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tasks/get",
        "params": {
            "id": run_id
        }
    });

    let get_resp = client
        .post(format!("{}/a2a", base_url))
        .json(&get_rpc)
        .send()
        .await
        .unwrap();

    let get_val: serde_json::Value = get_resp.json().await.unwrap();
    let get_task: A2aTask = serde_json::from_value(get_val["result"].clone()).unwrap();
    assert_eq!(get_task.id, run_id);
    assert_eq!(get_task.state, A2aTaskState::InputRequired);

    // 4. POST /a2a: tasks/list
    let list_rpc = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tasks/list",
        "params": {
            "status": "input-required"
        }
    });

    let list_resp = client
        .post(format!("{}/a2a", base_url))
        .json(&list_rpc)
        .send()
        .await
        .unwrap();

    let list_val: serde_json::Value = list_resp.json().await.unwrap();
    let tasks_list: Vec<A2aTask> = serde_json::from_value(list_val["result"].clone()).unwrap();
    assert!(!tasks_list.is_empty());
    assert!(tasks_list.iter().any(|t| t.id == run_id));

    // 5. Submit invalid finding (C002: Resolved with empty falsification_attempts) -> rejected
    let invalid_finding = CandidateFinding {
        candidate_id: Some("cand-invalid".to_string()),
        agent_id: Some("agent-test".to_string()),
        run_id: Some(run_id.clone()),
        storm_candidate_id: None,
        outcome: FindingOutcome::Resolved,
        primary_hypothesis: "Null camera pointer".to_string(),
        falsification_attempts: vec![], // Empty triggers C002!
        invalidated_hypotheses: vec![],
        failed_attempts: vec![],
        confidence_score: 0.9,
        remediation_proposal: "Check null pointer".to_string(),
        remediation_type: Some("permanent_fix".to_string()),
        supersedes_resolution_id: None,
        hold_for_curation: false,
        affected_symbols: vec!["com.motorola.camera.CameraApp.init".to_string()],
    };

    let invalid_submit_rpc = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tasks/send",
        "params": {
            "taskId": run_id,
            "finding": invalid_finding
        }
    });

    let invalid_resp = client
        .post(format!("{}/a2a", base_url))
        .json(&invalid_submit_rpc)
        .send()
        .await
        .unwrap();

    let invalid_val: serde_json::Value = invalid_resp.json().await.unwrap();
    let task_rejected: A2aTask = serde_json::from_value(invalid_val["result"].clone()).unwrap();
    assert_eq!(task_rejected.state, A2aTaskState::InputRequired);
    assert!(task_rejected.error.is_some());
    assert!(task_rejected.error.unwrap().contains("C002"));

    // 6. Submit valid finding -> Approved and Completed
    let valid_finding = CandidateFinding {
        candidate_id: Some("cand-valid".to_string()),
        agent_id: Some("agent-test".to_string()),
        run_id: Some(run_id.clone()),
        storm_candidate_id: None,
        outcome: FindingOutcome::Resolved,
        primary_hypothesis: "Null camera context pointer".to_string(),
        falsification_attempts: vec!["Tested null initialization under stress".to_string()],
        invalidated_hypotheses: vec![],
        failed_attempts: vec![],
        confidence_score: 0.95,
        remediation_proposal: "Add safe null check on CameraApp context".to_string(),
        remediation_type: Some("permanent_fix".to_string()),
        supersedes_resolution_id: None,
        hold_for_curation: false,
        affected_symbols: vec!["com.motorola.camera.CameraApp.init".to_string()],
    };

    let valid_submit_rpc = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tasks/send",
        "params": {
            "taskId": run_id,
            "finding": valid_finding
        }
    });

    let valid_resp = client
        .post(format!("{}/a2a", base_url))
        .json(&valid_submit_rpc)
        .send()
        .await
        .unwrap();

    let valid_val: serde_json::Value = valid_resp.json().await.unwrap();
    let task_approved: A2aTask = serde_json::from_value(valid_val["result"].clone()).unwrap();
    assert_eq!(task_approved.state, A2aTaskState::Completed);

    // Verify Vault nodes written on disk
    let inc_files: Vec<_> = fs::read_dir(vault_path.join("incidents"))
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert!(!inc_files.is_empty());

    let res_files: Vec<_> = fs::read_dir(vault_path.join("resolutions"))
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert!(!res_files.is_empty());

    // 7. POST /a2a: tasks/cancel on another task
    let cancel_trigger_rpc = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tasks/send",
        "params": {
            "jira_key": "INC-A2A-02",
            "app_id": "com.motorola.camera",
            "raw_logs": logs
        }
    });

    let cancel_trig_resp = client
        .post(format!("{}/a2a", base_url))
        .json(&cancel_trigger_rpc)
        .send()
        .await
        .unwrap();
    let cancel_trig_val: serde_json::Value = cancel_trig_resp.json().await.unwrap();
    let cancel_task: A2aTask = serde_json::from_value(cancel_trig_val["result"].clone()).unwrap();
    let cancel_id = cancel_task.id;

    let cancel_rpc = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": "tasks/cancel",
        "params": {
            "id": cancel_id
        }
    });

    let cancel_resp = client
        .post(format!("{}/a2a", base_url))
        .json(&cancel_rpc)
        .send()
        .await
        .unwrap();
    let cancel_val: serde_json::Value = cancel_resp.json().await.unwrap();
    let cancelled_task: A2aTask = serde_json::from_value(cancel_val["result"].clone()).unwrap();
    assert_eq!(cancelled_task.state, A2aTaskState::Canceled);
}
