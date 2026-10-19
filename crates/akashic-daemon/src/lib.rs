use akashic_engine::EngineConfig;
use akashic_rca::RcaConfig;
use akashic_vault::VaultConfig;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DaemonError {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Daemon network error: {0}")]
    Network(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    pub db_path: String,
    pub gerrit_host: String,
    pub gerrit_port: u16,
    pub debounce_quiet_secs: u64,
    pub debounce_max_cap_secs: u64,
    pub loop_drop_prefix: String,
    pub auto_discovery_enabled: bool,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            db_path: ".akashic/rca_state.db".to_string(),
            gerrit_host: "gerrit.mot.com".to_string(),
            gerrit_port: 29418,
            debounce_quiet_secs: 30,
            debounce_max_cap_secs: 300,
            loop_drop_prefix: "akashic/".to_string(),
            auto_discovery_enabled: true,
        }
    }
}

pub struct DaemonService {
    config: DaemonConfig,
    engine_config: EngineConfig,
    rca_config: RcaConfig,
    vault_config: VaultConfig,
}

impl DaemonService {
    pub fn new(
        config: DaemonConfig,
        engine_config: EngineConfig,
        rca_config: RcaConfig,
        vault_config: VaultConfig,
    ) -> Self {
        Self {
            config,
            engine_config,
            rca_config,
            vault_config,
        }
    }

    pub fn config(&self) -> &DaemonConfig {
        &self.config
    }

    pub fn engine_config(&self) -> &EngineConfig {
        &self.engine_config
    }

    pub fn rca_config(&self) -> &self::RcaConfig {
        &self.rca_config
    }

    pub fn vault_config(&self) -> &self::VaultConfig {
        &self.vault_config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_daemon_service_init() {
        let service = DaemonService::new(
            DaemonConfig::default(),
            EngineConfig::default(),
            RcaConfig::default(),
            VaultConfig::default(),
        );

        assert_eq!(service.config().gerrit_host, "gerrit.mot.com");
        assert_eq!(service.config().gerrit_port, 29418);
        assert!(service.config().auto_discovery_enabled);
    }
}
