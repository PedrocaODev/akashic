use akashic_embedding::EmbeddingProvider;
use akashic_sanitize::SanitizerProfile;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum VaultError {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Serialization(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultConfig {
    pub path: String,
    pub db_path: String,
    pub okf_version: String,
}

impl Default for VaultConfig {
    fn default() -> Self {
        Self {
            path: "vault".to_string(),
            db_path: ".akashic/vault_index.db".to_string(),
            okf_version: "0.2".to_string(),
        }
    }
}

pub struct VaultManager<S: SanitizerProfile, E: EmbeddingProvider> {
    config: VaultConfig,
    sanitizer: S,
    embedding: E,
}

impl<S: SanitizerProfile, E: EmbeddingProvider> VaultManager<S, E> {
    pub fn new(config: VaultConfig, sanitizer: S, embedding: E) -> Self {
        Self {
            config,
            sanitizer,
            embedding,
        }
    }

    pub fn config(&self) -> &VaultConfig {
        &self.config
    }

    pub fn sanitizer(&self) -> &S {
        &self.sanitizer
    }

    pub fn embedding(&self) -> &E {
        &self.embedding
    }

    pub fn sanitize_entry(&self, content: &str) -> akashic_sanitize::SanitizedOutput {
        self.sanitizer.sanitize(content)
    }

    pub fn is_embedding_available(&self) -> bool {
        self.embedding.is_available()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use akashic_embedding::OfflineEmbeddingProvider;
    use akashic_sanitize::CodeProfile;

    #[test]
    fn test_vault_dependency_wiring() {
        let config = VaultConfig::default();
        let sanitizer = CodeProfile::new();
        let embedding = OfflineEmbeddingProvider::default();
        let manager = VaultManager::new(config, sanitizer, embedding);

        let sanitized = manager.sanitize_entry("api_key = \"secret1234567890abcdef\"");
        assert!(sanitized.redactions_count > 0);
        assert!(!manager.is_embedding_available());
    }
}
