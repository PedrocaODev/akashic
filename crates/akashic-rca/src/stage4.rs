use crate::RcaError;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FindingOutcome {
    Resolved,
    Inconclusive,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HypothesisCandidate {
    pub hypothesis_id: String,
    pub description: String,
    pub falsification_test: String,
    pub confidence: f64,
    pub invalidated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CandidateFinding {
    #[serde(default)]
    pub candidate_id: Option<String>,
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub run_id: Option<String>,
    pub outcome: FindingOutcome,
    #[serde(alias = "summary")]
    pub primary_hypothesis: String,
    #[serde(default)]
    pub falsification_attempts: Vec<String>,
    #[serde(default)]
    pub invalidated_hypotheses: Vec<String>,
    #[serde(default)]
    pub failed_attempts: Vec<String>,
    #[serde(alias = "confidence")]
    pub confidence_score: f64,
    pub remediation_proposal: String,
    #[serde(default)]
    pub remediation_type: Option<String>,
    #[serde(default)]
    pub hold_for_curation: bool,
    #[serde(default)]
    pub affected_symbols: Vec<String>,
    #[serde(default)]
    pub supersedes_resolution_id: Option<String>,
    #[serde(default)]
    pub storm_candidate_id: Option<String>,
}

impl CandidateFinding {
    pub fn summary(&self) -> &str {
        &self.primary_hypothesis
    }

    pub fn candidate_id(&self) -> &str {
        self.candidate_id.as_deref().unwrap_or("")
    }
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

impl Stage4Output {
    pub fn into_candidate_finding(
        self,
        candidate_id: String,
        run_id: String,
        outcome: FindingOutcome,
        affected_symbols: Vec<String>,
        remediation_type: Option<String>,
        supersedes_resolution_id: Option<String>,
    ) -> CandidateFinding {
        let invalidated = self
            .invalidated_hypotheses
            .into_iter()
            .map(|h| format!("{}: {}", h.hypothesis_id, h.description))
            .collect();

        CandidateFinding {
            candidate_id: Some(candidate_id),
            agent_id: None,
            run_id: Some(run_id),
            outcome,
            primary_hypothesis: self.primary_hypothesis,
            falsification_attempts: self.falsification_attempts,
            invalidated_hypotheses: invalidated,
            failed_attempts: self.failed_attempts,
            confidence_score: self.confidence_score,
            remediation_proposal: self.remediation_proposal,
            remediation_type,
            hold_for_curation: false,
            affected_symbols,
            supersedes_resolution_id,
            storm_candidate_id: None,
        }
    }
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
