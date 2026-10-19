use crate::RcaError;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HypothesisCandidate {
    pub hypothesis_id: String,
    pub description: String,
    pub falsification_test: String,
    pub confidence: f64,
    pub invalidated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Stage4Output {
    #[serde(alias = "hypothesis")]
    pub primary_hypothesis: String,
    #[serde(default, alias = "falsification_results")]
    pub falsification_attempts: Vec<String>,
    #[serde(default)]
    pub invalidated_hypotheses: Vec<HypothesisCandidate>,
    #[serde(default)]
    pub failed_attempts: Vec<String>,
    #[serde(alias = "confidence")]
    pub confidence_score: f64,
    pub remediation_proposal: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stage4Request {
    #[serde(alias = "session_id")]
    pub run_id: String,
    pub jira_key: String,
    pub app_id: String,
    pub crash_signature: String,
    pub sanitized_logs: String,
    pub symbolication_evidence: serde_json::Value,
    pub policy_path: Option<PathBuf>,
}

pub type Stage4Input = Stage4Request;

pub trait Stage4Executor: Send + Sync {
    fn execute<'a>(
        &'a self,
        request: &'a Stage4Request,
    ) -> impl std::future::Future<Output = Result<Stage4Output, RcaError>> + Send;

    fn run_stage4<'a>(
        &'a self,
        request: &'a Stage4Request,
    ) -> impl std::future::Future<Output = Result<Stage4Output, RcaError>> + Send {
        self.execute(request)
    }
}

pub trait Stage4Runner: Send + Sync {
    fn run_stage4<'a>(
        &'a self,
        request: &'a Stage4Request,
    ) -> impl std::future::Future<Output = Result<Stage4Output, RcaError>> + Send;
}

impl<T: Stage4Executor> Stage4Runner for T {
    fn run_stage4<'a>(
        &'a self,
        request: &'a Stage4Request,
    ) -> impl std::future::Future<Output = Result<Stage4Output, RcaError>> + Send {
        self.execute(request)
    }
}

#[derive(Debug, Clone, Default)]
pub struct MockStage4Executor {
    pub custom_output: Option<Stage4Output>,
}

pub type MockStage4Runner = MockStage4Executor;

impl MockStage4Executor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_output(output: Stage4Output) -> Self {
        Self {
            custom_output: Some(output),
        }
    }
}

impl Stage4Executor for MockStage4Executor {
    async fn execute(&self, request: &Stage4Request) -> Result<Stage4Output, RcaError> {
        if let Some(ref output) = self.custom_output {
            return Ok(output.clone());
        }

        Ok(Stage4Output {
            primary_hypothesis: format!(
                "Null pointer dereference in {} caused by uninitialized client buffer",
                request.app_id
            ),
            falsification_attempts: vec![
                "Falsification attempt 1: checked if buffer can be initialized lazily (refuted)".to_string(),
                "Falsification attempt 2: verified null check prevents crash without side effects (confirmed)".to_string(),
            ],
            invalidated_hypotheses: vec![
                HypothesisCandidate {
                    hypothesis_id: "HYP-1".to_string(),
                    description: "Buffer corruption under multi-threaded contention".to_string(),
                    falsification_test: "Thread sanitizer concurrency stress run".to_string(),
                    confidence: 0.15,
                    invalidated: true,
                },
            ],
            failed_attempts: vec![
                "Attempted workaround via thread sleep yielded deadlock in soak testing".to_string(),
            ],
            confidence_score: 0.95,
            remediation_proposal: format!(
                "Add explicit null check and state validation before accessing client buffer in {}",
                request.app_id
            ),
        })
    }
}

#[derive(Debug, Clone)]
pub struct HeadlessGeminiExecutor {
    pub binary_path: PathBuf,
    pub model: String,
    pub default_policy_path: Option<PathBuf>,
}

pub type GeminiCliStage4Runner = HeadlessGeminiExecutor;
pub type CliSubprocessRunner = HeadlessGeminiExecutor;

impl Default for HeadlessGeminiExecutor {
    fn default() -> Self {
        Self {
            binary_path: PathBuf::from("gemini"),
            model: "gemini-2.5-pro".to_string(),
            default_policy_path: Some(PathBuf::from("config/policies/stage4-readonly.toml")),
        }
    }
}

impl HeadlessGeminiExecutor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_binary(binary_path: impl Into<PathBuf>) -> Self {
        Self {
            binary_path: binary_path.into(),
            ..Self::default()
        }
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    pub fn with_policy(mut self, policy_path: impl Into<PathBuf>) -> Self {
        self.default_policy_path = Some(policy_path.into());
        self
    }

    pub fn parse_gemini_output(raw_stdout: &str) -> Result<Stage4Output, RcaError> {
        let trimmed = raw_stdout.trim();
        if trimmed.is_empty() {
            return Err(RcaError::Stage4Failed("Empty stdout from gemini subprocess".to_string()));
        }

        // 1. Direct JSON parse
        if let Ok(output) = serde_json::from_str::<Stage4Output>(trimmed) {
            return Ok(output);
        }

        // 2. Extract ```json ... ``` blocks
        if let Some(start_idx) = trimmed.find("```json") {
            let after = &trimmed[start_idx + 7..];
            if let Some(end_idx) = after.find("```") {
                let code_content = after[..end_idx].trim();
                if let Ok(output) = serde_json::from_str::<Stage4Output>(code_content) {
                    return Ok(output);
                }
            }
        }

        // 3. Fallback: find first '{' and last '}'
        if let (Some(first_brace), Some(last_brace)) = (trimmed.find('{'), trimmed.rfind('}')) {
            if first_brace < last_brace {
                let json_slice = &trimmed[first_brace..=last_brace];
                if let Ok(output) = serde_json::from_str::<Stage4Output>(json_slice) {
                    return Ok(output);
                }
            }
        }

        Err(RcaError::Stage4Failed(format!(
            "Failed to parse Stage4Output JSON from subprocess stdout: {}",
            trimmed
        )))
    }
}

impl Stage4Executor for HeadlessGeminiExecutor {
    async fn execute(&self, request: &Stage4Request) -> Result<Stage4Output, RcaError> {
        let policy_to_use = request
            .policy_path
            .as_ref()
            .or(self.default_policy_path.as_ref());

        let prompt = format!(
            "Analyze the following Android crash and formulate an RCA root cause hypothesis, falsification attempts, and remediation proposal in JSON format.\n\n\
             Ticket: {}\n\
             App: {}\n\
             Crash Signature: {}\n\
             Symbolication Evidence: {}\n\
             Sanitized Logs:\n{}\n\n\
             Respond ONLY with a JSON object in the following format:\n\
             {{\n  \
               \"primary_hypothesis\": \"Detailed root cause explanation\",\n  \
               \"falsification_attempts\": [\"Test 1 and outcome\", \"Test 2 and outcome\"],\n  \
               \"invalidated_hypotheses\": [\n    \
                 {{\n      \
                   \"hypothesis_id\": \"HYP-1\",\n      \
                   \"description\": \"Candidate hypothesis\",\n      \
                   \"falsification_test\": \"Test procedure\",\n      \
                   \"confidence\": 0.1,\n      \
                   \"invalidated\": true\n    \
                 }}\n  \
               ],\n  \
               \"failed_attempts\": [\"Workaround attempt that failed\"],\n  \
               \"confidence_score\": 0.95,\n  \
               \"remediation_proposal\": \"Actionable fix recommendation\"\n\
             }}",
            request.jira_key,
            request.app_id,
            request.crash_signature,
            serde_json::to_string_pretty(&request.symbolication_evidence).unwrap_or_default(),
            request.sanitized_logs
        );

        let mut cmd = tokio::process::Command::new(&self.binary_path);
        cmd.arg("--approval-mode=yolo");

        if let Some(policy) = policy_to_use {
            cmd.arg("--policy").arg(policy);
        }

        cmd.arg("-p").arg(&prompt);

        let output = cmd.output().await.map_err(|e| {
            RcaError::Stage4Failed(format!("Failed to spawn gemini CLI process: {}", e))
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(RcaError::Stage4Failed(format!(
                "Gemini CLI process exited with error (code {:?}): {}",
                output.status.code(),
                stderr
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Self::parse_gemini_output(&stdout)
    }
}
