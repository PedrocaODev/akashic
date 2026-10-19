use akashic_engine::EngineConfig;
use akashic_lint::LintConfig;
use akashic_sanitize::CodeProfile;
use akashic_vault::VaultConfig;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RcaError {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Execution error: {0}")]
    Execution(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RcaConfig {
    pub db_path: String,
    pub symbol_store_path: String,
    pub symbol_cache_max_mb: usize,
    pub agent_cli: String,
    pub soak_dedup_enabled: bool,
}

impl Default for RcaConfig {
    fn default() -> Self {
        Self {
            db_path: ".akashic/rca_state.db".to_string(),
            symbol_store_path: ".akashic/symbols".to_string(),
            symbol_cache_max_mb: 2048,
            agent_cli: "gemini".to_string(),
            soak_dedup_enabled: true,
        }
    }
}

pub struct RcaPipeline {
    config: RcaConfig,
    vault_config: VaultConfig,
    lint_config: LintConfig,
    engine_config: EngineConfig,
    sanitizer: CodeProfile,
}

impl RcaPipeline {
    pub fn new(
        config: RcaConfig,
        vault_config: VaultConfig,
        lint_config: LintConfig,
        engine_config: EngineConfig,
    ) -> Self {
        Self {
            config,
            vault_config,
            lint_config,
            engine_config,
            sanitizer: CodeProfile::new(),
        }
    }

    pub fn config(&self) -> &RcaConfig {
        &self.config
    }

    pub fn vault_config(&self) -> &VaultConfig {
        &self.vault_config
    }

    pub fn lint_config(&self) -> &LintConfig {
        &self.lint_config
    }

    pub fn engine_config(&self) -> &EngineConfig {
        &self.engine_config
    }

    pub fn sanitizer(&self) -> &CodeProfile {
        &self.sanitizer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rca_pipeline_init() {
        let config = RcaConfig::default();
        let pipeline = RcaPipeline::new(
            config,
            VaultConfig::default(),
            LintConfig::default(),
            EngineConfig::default(),
        );

        assert_eq!(pipeline.config().agent_cli, "gemini");
        assert!(pipeline.config().soak_dedup_enabled);
    }
}
