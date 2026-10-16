use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SymbolicatorError {
    #[error("Failed to parse stack frame: {0}")]
    ParseError(String),
    #[error("Symbol resolution failed: {0}")]
    ResolutionFailed(String),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StackFrame {
    pub instruction_addr: u64,
    pub module_name: Option<String>,
    pub raw_symbol: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SymbolicatedFrame {
    pub instruction_addr: u64,
    pub function_name: Option<String>,
    pub file: Option<String>,
    pub line: Option<u32>,
    pub column: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SymbolicationRequest {
    pub request_id: String,
    pub frames: Vec<StackFrame>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SymbolicationResult {
    pub request_id: String,
    pub symbolicated_frames: Vec<SymbolicatedFrame>,
}
