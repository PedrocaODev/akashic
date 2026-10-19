pub mod db;
pub mod pipeline;
pub mod stage4;

pub use db::*;
pub use pipeline::*;
pub use stage4::*;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConductorError {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("Crystallizer error: {0}")]
    Crystallizer(#[from] rca_crystallizer::CrystallizerError),

    #[error("Symbolicator error: {0}")]
    Symbolicator(#[from] rca_symbolicator::SymbolicatorError),

    #[error("Sanitizer error: {0}")]
    Sanitizer(#[from] rca_sanitizer::SanitizerError),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Alexandria store error: {0}")]
    AlexandriaStore(#[from] alexandria_core::store::StoreError),

    #[error("Session error: {0}")]
    Session(String),

    #[error("Stage execution error: {0}")]
    StageFailed(String),

    #[error("Stage 4 execution failed: {0}")]
    Stage4Failed(String),

    #[error("Vault validation failed: {0} errors, {1} warnings")]
    VaultValidationFailed(usize, usize),

    #[error("Invalid state transition: from {from} to {to}")]
    InvalidStateTransition { from: String, to: String },

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
