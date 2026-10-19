use crate::ConductorError;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stage4Input {
    pub session_id: String,
    pub jira_key: String,
    pub app_id: String,
    pub crash_signature: String,
    pub sanitized_logs: String,
    pub symbolication_evidence: serde_json::Value,
    pub policy_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Stage4Output {
    pub hypothesis: String,
    pub falsification_results: Vec<String>,
    pub confidence: f64,
    pub remediation_proposal: String,
}

pub trait Stage4Runner: Send + Sync {
    fn run_stage4<'a>(
        &'a self,
        input: &'a Stage4Input,
    ) -> impl std::future::Future<Output = Result<Stage4Output, ConductorError>> + Send;
}

#[derive(Debug, Clone, Default)]
pub struct MockStage4Runner {
    pub custom_output: Option<Stage4Output>,
}

impl MockStage4Runner {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_output(output: Stage4Output) -> Self {
        Self {
            custom_output: Some(output),
        }
    }
}

impl Stage4Runner for MockStage4Runner {
    async fn run_stage4(&self, input: &Stage4Input) -> Result<Stage4Output, ConductorError> {
        if let Some(ref output) = self.custom_output {
            return Ok(output.clone());
        }

        Ok(Stage4Output {
            hypothesis: format!(
                "Null pointer dereference in {} caused by uninitialized client buffer",
                input.app_id
            ),
            falsification_results: vec![
                "Falsification attempt 1: checked if buffer can be initialized lazily (refuted)".to_string(),
                "Falsification attempt 2: verified null check prevents crash without side effects (confirmed)".to_string(),
            ],
            confidence: 0.95,
            remediation_proposal: format!(
                "Add explicit null check and state validation before accessing client buffer in {}",
                input.app_id
            ),
        })
    }
}

#[derive(Debug, Clone)]
pub struct CliSubprocessRunner {
    pub binary_path: PathBuf,
    pub model: String,
    pub default_policy_path: Option<PathBuf>,
}

impl Default for CliSubprocessRunner {
    fn default() -> Self {
        Self {
            binary_path: PathBuf::from("gemini"),
            model: "gemini-2.5-pro".to_string(),
            default_policy_path: Some(PathBuf::from("config/policies/stage4-readonly.toml")),
        }
    }
}

impl CliSubprocessRunner {
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

    pub fn parse_gemini_output(raw_stdout: &str) -> Result<Stage4Output, ConductorError> {
        let trimmed = raw_stdout.trim();
        if trimmed.is_empty() {
            return Err(ConductorError::Stage4Failed("Empty stdout from gemini subprocess".to_string()));
        }

        // 1. Try parsing directly as Stage4Output
        if let Ok(output) = serde_json::from_str::<Stage4Output>(trimmed) {
            return Ok(output);
        }

        // 2. Try parsing as Gemini CLI envelope: { "session_id": ..., "response": ... }
        #[derive(Deserialize)]
        struct Envelope {
            response: serde_json::Value,
        }

        if let Ok(env) = serde_json::from_str::<Envelope>(trimmed) {
            match env.response {
                serde_json::Value::String(s) => {
                    let cleaned = strip_markdown_fences(&s);
                    if let Ok(output) = serde_json::from_str::<Stage4Output>(cleaned) {
                        return Ok(output);
                    }
                }
                serde_json::Value::Object(_) => {
                    if let Ok(output) = serde_json::from_value::<Stage4Output>(env.response) {
                        return Ok(output);
                    }
                }
                _ => {}
            }
        }

        // 3. Try parsing after stripping markdown code blocks
        let cleaned = strip_markdown_fences(trimmed);
        if let Ok(output) = serde_json::from_str::<Stage4Output>(cleaned) {
            return Ok(output);
        }

        Err(ConductorError::Stage4Failed(format!(
            "Failed to parse Stage4Output from gemini stdout: {}",
            trimmed
        )))
    }
}

fn strip_markdown_fences(s: &str) -> &str {
    let mut trimmed = s.trim();
    if let Some(rest) = trimmed.strip_prefix("```json") {
        trimmed = rest;
    } else if let Some(rest) = trimmed.strip_prefix("```") {
        trimmed = rest;
    }
    if let Some(rest) = trimmed.strip_suffix("```") {
        trimmed = rest;
    }
    trimmed.trim()
}

impl Stage4Runner for CliSubprocessRunner {
    async fn run_stage4(&self, input: &Stage4Input) -> Result<Stage4Output, ConductorError> {
        use tokio::io::AsyncWriteExt;
        use tokio::process::Command;

        let policy_path = input
            .policy_path
            .clone()
            .or_else(|| self.default_policy_path.clone())
            .unwrap_or_else(|| PathBuf::from("config/policies/stage4-readonly.toml"));

        let prompt = "Execute Stage 4 RCA Hypothesis Generation and Adversarial Falsification. Return ONLY valid JSON matching schema: {\"hypothesis\": string, \"falsification_results\": string[], \"confidence\": number, \"remediation_proposal\": string}";

        let mut cmd = Command::new(&self.binary_path);
        cmd.arg("--output-format")
            .arg("json")
            .arg("--approval-mode")
            .arg("default")
            .arg("--skip-trust")
            .arg("-e")
            .arg("none")
            .arg("--allowed-mcp-server-names")
            .arg("akashic,alexandria,motojira-mcp,gerrit-mcp")
            .arg("--policy")
            .arg(policy_path.as_os_str())
            .arg("-m")
            .arg(&self.model)
            .arg("-p")
            .arg(prompt);

        cmd.env("GEMINI_CLI_TRUST_WORKSPACE", "true")
            .env("GEMINI_CLI_NO_RELAUNCH", "true")
            .env("NO_BROWSER", "true")
            .env("CI", "true")
            .env_remove("GEMINI_API_KEY")
            .env_remove("GOOGLE_API_KEY");

        cmd.stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| {
            ConductorError::Stage4Failed(format!(
                "Failed to spawn gemini CLI binary '{}': {}",
                self.binary_path.display(),
                e
            ))
        })?;

        let input_json = serde_json::to_string(input)?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(input_json.as_bytes()).await.map_err(|e| {
                ConductorError::Stage4Failed(format!("Failed writing to gemini stdin: {}", e))
            })?;
            stdin.shutdown().await.map_err(|e| {
                ConductorError::Stage4Failed(format!("Failed closing gemini stdin: {}", e))
            })?;
        }

        let output = child.wait_with_output().await.map_err(|e| {
            ConductorError::Stage4Failed(format!("Failed waiting for gemini CLI: {}", e))
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ConductorError::Stage4Failed(format!(
                "gemini CLI exited with status {}: {}",
                output.status, stderr
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Self::parse_gemini_output(&stdout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_stage4_runner_default_and_custom() {
        let runner = MockStage4Runner::new();
        let input = Stage4Input {
            session_id: "s1".to_string(),
            jira_key: "MOTOCAM-1".to_string(),
            app_id: "com.motorola.camera3".to_string(),
            crash_signature: "sig".to_string(),
            sanitized_logs: "logs".to_string(),
            symbolication_evidence: serde_json::Value::Null,
            policy_path: None,
        };

        let output = runner.run_stage4(&input).await.unwrap();
        assert!(output.hypothesis.contains("com.motorola.camera3"));
        assert_eq!(output.confidence, 0.95);

        // Custom output
        let custom = Stage4Output {
            hypothesis: "Custom test hypothesis".to_string(),
            falsification_results: vec!["Refuted".to_string()],
            confidence: 0.99,
            remediation_proposal: "Fix code".to_string(),
        };
        let custom_runner = MockStage4Runner::with_output(custom.clone());
        let res = custom_runner.run_stage4(&input).await.unwrap();
        assert_eq!(res, custom);
    }

    #[test]
    fn test_parse_gemini_output_direct_json() {
        let json = r#"{"hypothesis":"Direct NPE","falsification_results":["Passed"],"confidence":0.9,"remediation_proposal":"Guard check"}"#;
        let parsed = CliSubprocessRunner::parse_gemini_output(json).unwrap();
        assert_eq!(parsed.hypothesis, "Direct NPE");
        assert_eq!(parsed.confidence, 0.9);
    }

    #[test]
    fn test_parse_gemini_output_envelope_json() {
        let envelope = r#"{
            "session_id": "test-session",
            "response": "{\"hypothesis\":\"Envelope NPE\",\"falsification_results\":[\"Falsified\"],\"confidence\":0.88,\"remediation_proposal\":\"Add lock\"}"
        }"#;
        let parsed = CliSubprocessRunner::parse_gemini_output(envelope).unwrap();
        assert_eq!(parsed.hypothesis, "Envelope NPE");
        assert_eq!(parsed.confidence, 0.88);
    }

    #[test]
    fn test_parse_gemini_output_with_markdown_fences() {
        let envelope = r#"{
            "session_id": "test-session",
            "response": "```json\n{\"hypothesis\":\"Fenced NPE\",\"falsification_results\":[],\"confidence\":0.75,\"remediation_proposal\":\"Null check\"}\n```"
        }"#;
        let parsed = CliSubprocessRunner::parse_gemini_output(envelope).unwrap();
        assert_eq!(parsed.hypothesis, "Fenced NPE");
        assert_eq!(parsed.confidence, 0.75);
    }
}
