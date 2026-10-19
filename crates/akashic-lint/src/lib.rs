use akashic_vault::VaultConfig;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LintError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Parse error: {0}")]
    Parse(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LintConfig {
    pub enabled_rules: Vec<String>,
    pub strict_forward_only: bool,
    #[serde(default)]
    pub custom_fields: HashMap<String, String>,
}

impl Default for LintConfig {
    fn default() -> Self {
        Self {
            enabled_rules: vec![
                "L001".into(),
                "L002".into(),
                "L003".into(),
                "L004".into(),
                "L005".into(),
                "L006".into(),
                "L007".into(),
                "L008".into(),
                "L009".into(),
                "L010".into(),
                "L011".into(),
            ],
            strict_forward_only: true,
            custom_fields: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LintDiagnostic {
    pub rule: String,
    pub message: String,
    pub file_path: String,
    pub line: usize,
}

pub struct Linter {
    config: LintConfig,
    vault_config: VaultConfig,
}

impl Linter {
    pub fn new(config: LintConfig, vault_config: VaultConfig) -> Self {
        Self {
            config,
            vault_config,
        }
    }

    pub fn config(&self) -> &LintConfig {
        &self.config
    }

    pub fn vault_config(&self) -> &VaultConfig {
        &self.vault_config
    }

    pub fn check_content(&self, file_path: &str, content: &str) -> Vec<LintDiagnostic> {
        let mut diagnostics = Vec::new();
        if self.config.enabled_rules.contains(&"L001".to_string()) && !content.starts_with("---") {
            diagnostics.push(LintDiagnostic {
                rule: "L001".to_string(),
                message: "Missing frontmatter delimiter".to_string(),
                file_path: file_path.to_string(),
                line: 1,
            });
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lint_rules() {
        let config = LintConfig::default();
        let vault_config = VaultConfig::default();
        let linter = Linter::new(config, vault_config);

        let diags = linter.check_content("test.md", "hello world");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule, "L001");
    }
}
