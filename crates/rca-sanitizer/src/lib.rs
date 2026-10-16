use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SanitizerError {
    #[error("Sanitization processing failed: {0}")]
    ProcessingFailed(String),
    #[error("Invalid input format: {0}")]
    InvalidInput(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SanitizedOutput {
    pub sanitized_content: String,
    pub redactions_count: usize,
}

pub trait Sanitizer {
    type Error: std::error::Error + Send + Sync + 'static;

    fn sanitize(&self, raw: &str) -> Result<SanitizedOutput, Self::Error>;
}

#[derive(Debug, Default, Clone)]
pub struct BasicSanitizer;

impl Sanitizer for BasicSanitizer {
    type Error = SanitizerError;

    fn sanitize(&self, raw: &str) -> Result<SanitizedOutput, Self::Error> {
        Ok(SanitizedOutput {
            sanitized_content: raw.to_string(),
            redactions_count: 0,
        })
    }
}
