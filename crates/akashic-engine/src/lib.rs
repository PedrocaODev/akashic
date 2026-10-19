use akashic_embedding::EmbeddingProvider;
use akashic_sanitize::SanitizerProfile;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Parse error: {0}")]
    Parse(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineConfig {
    pub db_path: String,
    pub languages: Vec<String>,
    pub inferred_similarity_threshold: f64,
    pub god_node_degree_threshold: usize,
    pub rrf_k: usize,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            db_path: ".akashic/code_graph.db".to_string(),
            languages: vec![
                "java".into(),
                "kotlin".into(),
                "cpp".into(),
                "rust".into(),
                "python".into(),
                "go".into(),
                "typescript".into(),
            ],
            inferred_similarity_threshold: 0.82,
            god_node_degree_threshold: 25,
            rrf_k: 60,
        }
    }
}

pub struct CodeGraphEngine<S: SanitizerProfile, E: EmbeddingProvider> {
    config: EngineConfig,
    sanitizer: S,
    embedding: E,
}

impl<S: SanitizerProfile, E: EmbeddingProvider> CodeGraphEngine<S, E> {
    pub fn new(config: EngineConfig, sanitizer: S, embedding: E) -> Self {
        Self {
            config,
            sanitizer,
            embedding,
        }
    }

    pub fn config(&self) -> &EngineConfig {
        &self.config
    }

    pub fn sanitizer(&self) -> &S {
        &self.sanitizer
    }

    pub fn embedding(&self) -> &E {
        &self.embedding
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use akashic_embedding::OfflineEmbeddingProvider;
    use akashic_sanitize::CodeProfile;

    #[test]
    fn test_engine_init() {
        let config = EngineConfig::default();
        let sanitizer = CodeProfile::new();
        let embedding = OfflineEmbeddingProvider::default();
        let engine = CodeGraphEngine::new(config, sanitizer, embedding);

        assert_eq!(engine.config().god_node_degree_threshold, 25);
        assert!(!engine.embedding().is_available());
    }
}
