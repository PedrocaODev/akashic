use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConductorError {
    #[error("Session error: {0}")]
    Session(String),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Invalid state transition: from {from:?} to {to:?}")]
    InvalidStateTransition {
        from: RunStatus,
        to: RunStatus,
    },
    #[error("Pipeline execution failed: {0}")]
    ExecutionFailed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisSession {
    pub session_id: String,
    pub target: String,
    pub status: RunStatus,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationState {
    pub session: AnalysisSession,
    pub step_index: usize,
    pub logs: Vec<String>,
}

impl AnalysisSession {
    pub fn new(session_id: impl Into<String>, target: impl Into<String>, timestamp: u64) -> Self {
        Self {
            session_id: session_id.into(),
            target: target.into(),
            status: RunStatus::Pending,
            created_at: timestamp,
            updated_at: timestamp,
        }
    }

    pub fn transition_to(&mut self, next_status: RunStatus, timestamp: u64) -> Result<(), ConductorError> {
        self.status = next_status;
        self.updated_at = timestamp;
        Ok(())
    }
}
