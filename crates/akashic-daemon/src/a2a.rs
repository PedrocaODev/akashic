use crate::DaemonError;
use akashic_rca::{
    CandidateFinding, ConductorPipeline, CurationReport, CurationVerdict,
    MockStage4Executor, PipelineInput, RcaError, StateStore, WorkflowRunRecord,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct A2aAgentCard {
    pub name: String,
    pub version: String,
    pub description: String,
    pub protocol_version: String,
    pub endpoints: Vec<String>,
    pub skills: Vec<String>,
    pub capabilities: serde_json::Value,
}

impl Default for A2aAgentCard {
    fn default() -> Self {
        Self {
            name: "akashic-rca".to_string(),
            version: "1.0.0".to_string(),
            description: "Akashic Two-Phase RCA & Code Investigation Agent".to_string(),
            protocol_version: "1.0".to_string(),
            endpoints: vec!["/a2a".to_string()],
            skills: vec![
                "prepare_investigation".to_string(),
                "get_investigation_packet".to_string(),
                "submit_finding".to_string(),
                "curate_investigation".to_string(),
                "query_code_graph".to_string(),
                "query_vault".to_string(),
            ],
            capabilities: serde_json::json!({
                "streaming": false,
                "push_notifications": false,
            }),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum A2aTaskState {
    Submitted,
    Working,
    InputRequired,
    Completed,
    Failed,
    Canceled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct A2aArtifact {
    pub id: String,
    #[serde(rename = "mimeType", alias = "mime_type")]
    pub mime_type: String,
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct A2aTask {
    pub id: String,
    #[serde(rename = "sessionId", skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    pub state: A2aTaskState,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_packet: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub curation_report: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifacts: Option<Vec<A2aArtifact>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct A2aServerConfig {
    pub bind_addr: String,
    pub db_path: PathBuf,
    pub vault_path: PathBuf,
    pub graph_store: Option<PathBuf>,
    pub policy_path: Option<PathBuf>,
}

impl Default for A2aServerConfig {
    fn default() -> Self {
        Self {
            bind_addr: "127.0.0.1:9091".to_string(),
            db_path: PathBuf::from(".akashic/rca_state.db"),
            vault_path: PathBuf::from("vault"),
            graph_store: None,
            policy_path: None,
        }
    }
}

pub struct A2aServer {
    config: A2aServerConfig,
    state_store: StateStore,
    pipeline: ConductorPipeline<MockStage4Executor>,
}

impl A2aServer {
    pub fn new(config: A2aServerConfig) -> Result<Self, DaemonError> {
        if let Some(parent) = config.db_path.parent() {
            if !parent.as_os_str().is_empty() {
                let _ = std::fs::create_dir_all(parent);
            }
        }
        let state_store = StateStore::open(&config.db_path)
            .map_err(|e| DaemonError::Database(rusqlite::Error::ToSqlConversionFailure(Box::new(e))))?;
        let pipeline = ConductorPipeline::new(state_store.clone(), MockStage4Executor::new());
        Ok(Self {
            config,
            state_store,
            pipeline,
        })
    }

    pub fn with_state_store(config: A2aServerConfig, state_store: StateStore) -> Self {
        let pipeline = ConductorPipeline::new(state_store.clone(), MockStage4Executor::new());
        Self {
            config,
            state_store,
            pipeline,
        }
    }

    pub fn agent_card(&self) -> A2aAgentCard {
        A2aAgentCard::default()
    }

    pub fn config(&self) -> &A2aServerConfig {
        &self.config
    }

    pub fn state_store(&self) -> &StateStore {
        &self.state_store
    }

    pub fn handle_jsonrpc(&self, req_json: &str) -> String {
        let req: serde_json::Value = match serde_json::from_str(req_json) {
            Ok(v) => v,
            Err(e) => {
                return serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": null,
                    "error": {
                        "code": -32700,
                        "message": format!("Parse error: {}", e)
                    }
                })
                .to_string();
            }
        };

        let id = req.get("id").cloned().unwrap_or(serde_json::Value::Null);
        let method = match req.get("method").and_then(|m| m.as_str()) {
            Some(m) => m,
            None => {
                return serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": {
                        "code": -32600,
                        "message": "Invalid Request: missing method"
                    }
                })
                .to_string();
            }
        };

        let params = req.get("params").cloned().unwrap_or(serde_json::Value::Null);

        let result_or_err = match method {
            "tasks/list" => self.handle_tasks_list(&params),
            "tasks/get" => self.handle_tasks_get(&params),
            "tasks/send" | "message/send" => self.handle_tasks_send(&params),
            "tasks/cancel" => self.handle_tasks_cancel(&params),
            _ => Err((-32601, format!("Method not found: {}", method))),
        };

        match result_or_err {
            Ok(res) => serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": res
            })
            .to_string(),
            Err((code, msg)) => serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {
                    "code": code,
                    "message": msg
                }
            })
            .to_string(),
        }
    }

    fn build_a2a_task(
        &self,
        run: &WorkflowRunRecord,
        report: Option<&CurationReport>,
    ) -> Result<A2aTask, RcaError> {
        let state = match run.status.as_str() {
            "AWAITING_REASONING" | "AWAITING_CURATION" => A2aTaskState::InputRequired,
            "IN_PROGRESS" => match run.current_stage.as_str() {
                "AWAITING_REASONING" | "AWAITING_CURATION" => A2aTaskState::InputRequired,
                _ => A2aTaskState::Working,
            },
            "COMPLETED" | "RESOLVED" | "CRYSTALLIZED" | "INCONCLUSIVE" | "DEDUPLICATED" => {
                A2aTaskState::Completed
            }
            "FAILED" => A2aTaskState::Failed,
            "CANCELED" => A2aTaskState::Canceled,
            _ => A2aTaskState::Working,
        };

        let context_packet_val: Option<serde_json::Value> = match run.context_packet_json.as_deref() {
            Some(json) => serde_json::from_str(json).ok(),
            None => self
                .state_store
                .get_context_packet(&run.run_id)?
                .and_then(|json| serde_json::from_str(&json).ok()),
        };

        let mut artifacts = Vec::new();
        if let Some(ref pkt) = context_packet_val {
            artifacts.push(A2aArtifact {
                id: format!("{}-context-packet", run.run_id),
                mime_type: "application/json".to_string(),
                data: pkt.clone(),
            });
        }

        let curation_report_val = report.and_then(|r| serde_json::to_value(r).ok());
        if let Some(ref rep) = curation_report_val {
            artifacts.push(A2aArtifact {
                id: format!("{}-curation-report", run.run_id),
                mime_type: "application/json".to_string(),
                data: rep.clone(),
            });
        }

        let candidates = self
            .state_store
            .list_candidate_findings(&run.run_id)
            .unwrap_or_default();
        let candidates_val = serde_json::to_value(&candidates).ok();

        let result_val = if let Some(ref rep) = curation_report_val {
            Some(serde_json::json!({
                "status": run.status,
                "curation_report": rep,
                "candidate_findings": candidates_val,
            }))
        } else if !candidates.is_empty() {
            Some(serde_json::json!({
                "status": run.status,
                "candidate_findings": candidates_val,
            }))
        } else {
            None
        };

        let error_msg = if let Some(rep) = report {
            if rep.verdict == CurationVerdict::Rejected {
                Some(
                    rep.issues
                        .iter()
                        .map(|i| format!("{}: {}", i.invariant, i.message))
                        .collect::<Vec<_>>()
                        .join("; "),
                )
            } else {
                None
            }
        } else if state == A2aTaskState::Failed {
            Some(run.current_stage.clone())
        } else {
            None
        };

        Ok(A2aTask {
            id: run.run_id.clone(),
            session_id: None,
            state,
            created_at: run.created_at,
            updated_at: run.updated_at,
            context_packet: context_packet_val,
            curation_report: curation_report_val,
            artifacts: if artifacts.is_empty() {
                None
            } else {
                Some(artifacts)
            },
            result: result_val,
            error: error_msg,
        })
    }

    fn handle_tasks_list(&self, params: &serde_json::Value) -> Result<serde_json::Value, (i64, String)> {
        let limit = params
            .get("limit")
            .and_then(|l| l.as_u64())
            .unwrap_or(50) as usize;
        let status_filter = params
            .get("status")
            .and_then(|s| s.as_str());

        let runs = self
            .state_store
            .list_workflow_runs()
            .map_err(|e| (-32000, format!("Failed to list runs: {}", e)))?;

        let mut tasks = Vec::new();
        for run in runs {
            if let Ok(task) = self.build_a2a_task(&run, None) {
                if let Some(filter) = status_filter {
                    let task_state_str = serde_json::to_string(&task.state)
                        .unwrap_or_default()
                        .trim_matches('"')
                        .to_string();
                    if task_state_str != filter {
                        continue;
                    }
                }
                tasks.push(task);
                if tasks.len() >= limit {
                    break;
                }
            }
        }

        Ok(serde_json::to_value(tasks).unwrap_or(serde_json::json!([])))
    }

    fn handle_tasks_get(&self, params: &serde_json::Value) -> Result<serde_json::Value, (i64, String)> {
        let id = params
            .get("id")
            .or_else(|| params.get("taskId"))
            .or_else(|| params.get("task_id"))
            .or_else(|| params.get("run_id"))
            .and_then(|i| i.as_str())
            .or_else(|| params.as_str())
            .ok_or_else(|| (-32602, "Missing 'id' parameter".to_string()))?;

        let run = self
            .state_store
            .get_workflow_run(id)
            .map_err(|e| (-32000, format!("Database error: {}", e)))?
            .ok_or_else(|| (-32602, format!("Task '{}' not found", id)))?;

        let task = self
            .build_a2a_task(&run, None)
            .map_err(|e| (-32000, format!("Error building task: {}", e)))?;

        Ok(serde_json::to_value(task).unwrap_or(serde_json::Value::Null))
    }

    fn handle_tasks_cancel(&self, params: &serde_json::Value) -> Result<serde_json::Value, (i64, String)> {
        let id = params
            .get("id")
            .or_else(|| params.get("taskId"))
            .or_else(|| params.get("task_id"))
            .or_else(|| params.get("run_id"))
            .and_then(|i| i.as_str())
            .or_else(|| params.as_str())
            .ok_or_else(|| (-32602, "Missing 'id' parameter".to_string()))?;

        let run = self
            .state_store
            .get_workflow_run(id)
            .map_err(|e| (-32000, format!("Database error: {}", e)))?
            .ok_or_else(|| (-32602, format!("Task '{}' not found", id)))?;

        self.state_store
            .update_workflow_run(id, "CANCELED", "CANCELED", run.crash_signature.as_deref(), &run.context_json)
            .map_err(|e| (-32000, format!("Failed to cancel task: {}", e)))?;

        let updated_run = self
            .state_store
            .get_workflow_run(id)
            .map_err(|e| (-32000, format!("Database error: {}", e)))?
            .unwrap_or(run);

        let task = self
            .build_a2a_task(&updated_run, None)
            .map_err(|e| (-32000, format!("Error building task: {}", e)))?;

        Ok(serde_json::to_value(task).unwrap_or(serde_json::Value::Null))
    }

    fn handle_tasks_send(&self, params: &serde_json::Value) -> Result<serde_json::Value, (i64, String)> {
        let run_id_opt = params
            .get("taskId")
            .or_else(|| params.get("task_id"))
            .or_else(|| params.get("run_id"))
            .or_else(|| params.get("id"))
            .and_then(|v| v.as_str());

        // Check if winning_candidate_id adjudication is requested
        if let Some(winning_cand) = params
            .get("winning_candidate_id")
            .or_else(|| params.get("candidate_id"))
            .and_then(|v| v.as_str())
        {
            let run_id = run_id_opt.ok_or_else(|| {
                (-32602, "Missing 'taskId' or 'run_id' for curation".to_string())
            })?;

            let (_ctx, report) = self
                .pipeline
                .curate_investigation(run_id, winning_cand, &self.config.vault_path)
                .map_err(|e| (-32000, format!("Curation adjudication failed: {}", e)))?;

            let run = self
                .state_store
                .get_workflow_run(run_id)
                .map_err(|e| (-32000, format!("Database error: {}", e)))?
                .ok_or_else(|| (-32602, format!("Run '{}' not found", run_id)))?;

            let task = self
                .build_a2a_task(&run, Some(&report))
                .map_err(|e| (-32000, format!("Error building task: {}", e)))?;

            return Ok(serde_json::to_value(task).unwrap_or(serde_json::Value::Null));
        }

        // Check if CandidateFinding submission is requested
        let finding_val = params
            .get("finding")
            .or_else(|| params.get("candidate_finding"))
            .or_else(|| params.get("data"))
            .or_else(|| params.get("data_part"))
            .or_else(|| {
                if params.get("primary_hypothesis").is_some() {
                    Some(params)
                } else {
                    None
                }
            });

        if let (Some(run_id), Some(finding_json)) = (run_id_opt, finding_val) {
            // Check if finding is valid CandidateFinding
            if let Ok(mut finding) = serde_json::from_value::<CandidateFinding>(finding_json.clone()) {
                if finding.run_id.is_none() {
                    finding.run_id = Some(run_id.to_string());
                }

                let (_ctx, report) = self
                    .pipeline
                    .submit_finding(run_id, &finding, &self.config.vault_path)
                    .map_err(|e| (-32000, format!("Submit finding failed: {}", e)))?;

                let run = self
                    .state_store
                    .get_workflow_run(run_id)
                    .map_err(|e| (-32000, format!("Database error: {}", e)))?
                    .ok_or_else(|| (-32602, format!("Run '{}' not found", run_id)))?;

                let task = self
                    .build_a2a_task(&run, Some(&report))
                    .map_err(|e| (-32000, format!("Error building task: {}", e)))?;

                return Ok(serde_json::to_value(task).unwrap_or(serde_json::Value::Null));
            }
        }

        // Otherwise, this is Phase A: prepare_investigation
        let jira_key = params
            .get("jira_key")
            .or_else(|| params.get("jiraKey"))
            .and_then(|v| v.as_str())
            .unwrap_or("INC-UNKNOWN")
            .to_string();

        let app_id = params
            .get("app_id")
            .or_else(|| params.get("appId"))
            .and_then(|v| v.as_str())
            .unwrap_or("com.unknown.app")
            .to_string();

        let device = params
            .get("device")
            .and_then(|v| v.as_str())
            .unwrap_or("generic")
            .to_string();

        let android_version = params
            .get("android_version")
            .or_else(|| params.get("androidVersion"))
            .and_then(|v| v.as_str())
            .unwrap_or("14")
            .to_string();

        let raw_logs = params
            .get("raw_logs")
            .or_else(|| params.get("rawLogs"))
            .or_else(|| params.get("logs"))
            .or_else(|| params.get("text"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let vault_path = params
            .get("vault_path")
            .or_else(|| params.get("vaultPath"))
            .and_then(|v| v.as_str())
            .map(PathBuf::from)
            .unwrap_or_else(|| self.config.vault_path.clone());

        let graph_store = params
            .get("graph_store")
            .or_else(|| params.get("graphStore"))
            .and_then(|v| v.as_str())
            .map(PathBuf::from)
            .or_else(|| self.config.graph_store.clone());

        let prior_resolution_id = params
            .get("prior_resolution_id")
            .or_else(|| params.get("priorResolutionId"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let policy_path = params
            .get("policy_path")
            .or_else(|| params.get("policyPath"))
            .and_then(|v| v.as_str())
            .map(PathBuf::from)
            .or_else(|| self.config.policy_path.clone());

        let input = PipelineInput {
            run_id: run_id_opt.map(|s| s.to_string()),
            legacy_session_id: None,
            jira_key,
            app_id,
            device,
            android_version,
            raw_logs,
            vault_path,
            graph_store,
            prior_resolution_id,
            policy_path,
        };

        let (ctx, _packet) = self
            .pipeline
            .prepare_investigation(input)
            .map_err(|e| (-32000, format!("prepare_investigation failed: {}", e)))?;

        let run = self
            .state_store
            .get_workflow_run(&ctx.run_id)
            .map_err(|e| (-32000, format!("Database error: {}", e)))?
            .ok_or_else(|| (-32000, format!("Run '{}' not created", ctx.run_id)))?;

        let task = self
            .build_a2a_task(&run, None)
            .map_err(|e| (-32000, format!("Error building task: {}", e)))?;

        Ok(serde_json::to_value(task).unwrap_or(serde_json::Value::Null))
    }

    pub fn handle_http_request(&self, req_bytes: &[u8]) -> (u16, &'static str, String) {
        let req_str = match std::str::from_utf8(req_bytes) {
            Ok(s) => s,
            Err(_) => return (400, "text/plain", "Bad Request: invalid UTF-8".to_string()),
        };

        let mut lines = req_str.lines();
        let request_line = match lines.next() {
            Some(l) => l,
            None => return (400, "text/plain", "Bad Request: empty".to_string()),
        };

        let mut parts = request_line.split_whitespace();
        let method = parts.next().unwrap_or("");
        let path = parts.next().unwrap_or("");

        if method == "OPTIONS" {
            return (200, "text/plain", "OK".to_string());
        }

        if method == "GET" && (path == "/.well-known/agent.json" || path == "/agent.json") {
            let card = self.agent_card();
            let body = serde_json::to_string_pretty(&card).unwrap_or_default();
            return (200, "application/json", body);
        }

        if method == "POST" && (path == "/a2a" || path == "/" || path == "/rpc") {
            let body = if let Some(pos) = req_str.find("\r\n\r\n") {
                &req_str[pos + 4..]
            } else if let Some(pos) = req_str.find("\n\n") {
                &req_str[pos + 2..]
            } else {
                ""
            };

            let resp_json = self.handle_jsonrpc(body);
            return (200, "application/json", resp_json);
        }

        (404, "text/plain", "Not Found".to_string())
    }

    pub async fn serve(self: Arc<Self>, listener: tokio::net::TcpListener) -> Result<(), DaemonError> {
        loop {
            let (mut socket, _) = listener.accept().await?;
            let server = Arc::clone(&self);

            tokio::spawn(async move {
                let mut buf = vec![0u8; 65536];
                let mut total_read = 0;

                loop {
                    match socket.read(&mut buf[total_read..]).await {
                        Ok(0) => break,
                        Ok(n) => {
                            total_read += n;
                            let req_slice = &buf[..total_read];
                            let header_end = req_slice
                                .windows(4)
                                .position(|w| w == b"\r\n\r\n")
                                .map(|pos| pos + 4)
                                .or_else(|| req_slice.windows(2).position(|w| w == b"\n\n").map(|p| p + 2));

                            if let Some(h_end) = header_end {
                                let header_str = String::from_utf8_lossy(&req_slice[..h_end]);
                                let mut content_len = 0;
                                for line in header_str.lines() {
                                    if line.to_ascii_lowercase().starts_with("content-length:") {
                                        if let Some(len_str) = line.split(':').nth(1) {
                                            content_len = len_str.trim().parse::<usize>().unwrap_or(0);
                                        }
                                    }
                                }

                                if total_read >= h_end + content_len {
                                    let (status, mime, body) = server.handle_http_request(&buf[..total_read]);
                                    let status_text = match status {
                                        200 => "200 OK",
                                        400 => "400 Bad Request",
                                        404 => "404 Not Found",
                                        _ => "500 Internal Server Error",
                                    };
                                    let resp = format!(
                                        "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n{}",
                                        status_text,
                                        mime,
                                        body.len(),
                                        body
                                    );
                                    let _ = socket.write_all(resp.as_bytes()).await;
                                    let _ = socket.flush().await;
                                    break;
                                }
                            }
                        }
                        Err(_) => break,
                    }

                    if total_read >= buf.len() {
                        buf.resize(buf.len() * 2, 0);
                    }
                }
            });
        }
    }
}
