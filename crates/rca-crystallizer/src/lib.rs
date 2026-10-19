use akashic::graph::VaultGraph;
use akashic::linter::lint_vault;
use akashic::model::{LintReport, Node, Relation, RelationType, Scope};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CrystallizerError {
    #[error("Crystallization failed: {0}")]
    Failed(String),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("YAML serialization error: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Invalid node structure: {0}")]
    InvalidNode(String),
    #[error("Forward-only rule violation: {0}")]
    ForwardOnlyViolation(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrystallizedArtifact {
    pub root_node: Node,
    pub auxiliary_relations: Vec<Relation>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExternalRefs {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jira_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bug2go_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GerritClRef {
    pub repo: String,
    pub change_id: String,
    pub cl_number: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResolutionExternalRefs {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jira_key: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gerrit_cls: Vec<GerritClRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CrystallizerRelation {
    #[serde(rename = "type")]
    pub relation_type: String,
    pub target: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence: Option<String>,
}

pub fn validate_relations(relations: &[CrystallizerRelation]) -> Result<(), CrystallizerError> {
    for rel in relations {
        let t = rel.relation_type.trim().to_lowercase();
        if t == "resolved_by" {
            return Err(CrystallizerError::ForwardOnlyViolation(
                "Relation 'resolved_by' violates the forward-only rule. Resolutions must declare forward 'resolves' instead."
                    .to_string(),
            ));
        }
        if t == "superseded_by" || t == "required_by" || t == "supports" || t == "caused" {
            return Err(CrystallizerError::ForwardOnlyViolation(format!(
                "Inverse relation '{}' cannot be declared in frontmatter (violates forward-only rule)",
                rel.relation_type
            )));
        }
    }
    Ok(())
}

pub fn format_markdown_note<T: Serialize>(
    frontmatter: &T,
    body: &str,
) -> Result<String, CrystallizerError> {
    let yaml = serde_yaml::to_string(frontmatter)?;
    let yaml_trimmed = yaml.trim_start_matches("---\n").trim();
    Ok(format!("---\n{}\n---\n\n{}\n", yaml_trimmed, body.trim()))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IncidentFrontmatter {
    pub id: String,
    #[serde(rename = "type")]
    pub node_type: String,
    pub title: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_from: Option<String>,
    pub crash_signature: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub scope: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_refs: Option<ExternalRefs>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relations: Vec<CrystallizerRelation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncidentNode {
    pub id: String,
    pub title: String,
    pub status: String,
    pub valid_from: Option<String>,
    pub crash_signature: String,
    pub scope: BTreeMap<String, String>,
    pub external_refs: Option<ExternalRefs>,
    pub relations: Vec<CrystallizerRelation>,
    pub body: String,
}

impl IncidentNode {
    pub fn validate_forward_only(&self) -> Result<(), CrystallizerError> {
        validate_relations(&self.relations)
    }

    pub fn to_markdown(&self) -> Result<String, CrystallizerError> {
        self.validate_forward_only()?;
        let frontmatter = IncidentFrontmatter {
            id: self.id.clone(),
            node_type: "Incident".to_string(),
            title: self.title.clone(),
            status: self.status.clone(),
            valid_from: self.valid_from.clone(),
            crash_signature: self.crash_signature.clone(),
            scope: self.scope.clone(),
            external_refs: self.external_refs.clone(),
            relations: self.relations.clone(),
        };
        format_markdown_note(&frontmatter, &self.body)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvidenceFrontmatter {
    pub id: String,
    #[serde(rename = "type")]
    pub node_type: String,
    pub title: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub scope: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_refs: Option<ExternalRefs>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relations: Vec<CrystallizerRelation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceNode {
    pub id: String,
    pub title: String,
    pub status: String,
    pub scope: BTreeMap<String, String>,
    pub external_refs: Option<ExternalRefs>,
    pub relations: Vec<CrystallizerRelation>,
    pub body: String,
}

impl EvidenceNode {
    pub fn validate_forward_only(&self) -> Result<(), CrystallizerError> {
        validate_relations(&self.relations)
    }

    pub fn to_markdown(&self) -> Result<String, CrystallizerError> {
        self.validate_forward_only()?;
        let frontmatter = EvidenceFrontmatter {
            id: self.id.clone(),
            node_type: "Evidence".to_string(),
            title: self.title.clone(),
            status: self.status.clone(),
            scope: self.scope.clone(),
            external_refs: self.external_refs.clone(),
            relations: self.relations.clone(),
        };
        format_markdown_note(&frontmatter, &self.body)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResolutionFrontmatter {
    pub id: String,
    #[serde(rename = "type")]
    pub node_type: String,
    pub title: String,
    pub status: String,
    pub remediation_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub abandoned_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_refs: Option<ResolutionExternalRefs>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relations: Vec<CrystallizerRelation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolutionNode {
    pub id: String,
    pub title: String,
    pub status: String,
    pub remediation_type: String,
    pub abandoned_reason: Option<String>,
    pub external_refs: Option<ResolutionExternalRefs>,
    pub relations: Vec<CrystallizerRelation>,
    pub body: String,
}

impl ResolutionNode {
    pub fn validate_forward_only(&self) -> Result<(), CrystallizerError> {
        validate_relations(&self.relations)
    }

    pub fn to_markdown(&self) -> Result<String, CrystallizerError> {
        self.validate_forward_only()?;
        let frontmatter = ResolutionFrontmatter {
            id: self.id.clone(),
            node_type: "Resolution".to_string(),
            title: self.title.clone(),
            status: self.status.clone(),
            remediation_type: self.remediation_type.clone(),
            abandoned_reason: self.abandoned_reason.clone(),
            external_refs: self.external_refs.clone(),
            relations: self.relations.clone(),
        };
        format_markdown_note(&frontmatter, &self.body)
    }
}

pub struct CrystallizerWriter {
    vault_root: PathBuf,
}

impl CrystallizerWriter {
    pub fn new(vault_root: impl Into<PathBuf>) -> Self {
        Self {
            vault_root: vault_root.into(),
        }
    }

    pub fn write_incident(
        &self,
        node: &IncidentNode,
    ) -> Result<PathBuf, CrystallizerError> {
        node.validate_forward_only()?;
        let content = node.to_markdown()?;
        let dir = self.vault_root.join("incidents");
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{}.md", node.id));
        std::fs::write(&path, content)?;
        Ok(path)
    }

    pub fn write_evidence(
        &self,
        node: &EvidenceNode,
    ) -> Result<PathBuf, CrystallizerError> {
        node.validate_forward_only()?;
        let content = node.to_markdown()?;
        let dir = self.vault_root.join("evidence");
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{}.md", node.id));
        std::fs::write(&path, content)?;
        Ok(path)
    }

    pub fn write_resolution(
        &self,
        node: &ResolutionNode,
    ) -> Result<PathBuf, CrystallizerError> {
        node.validate_forward_only()?;
        let content = node.to_markdown()?;
        let dir = self.vault_root.join("resolutions");
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{}.md", node.id));
        std::fs::write(&path, content)?;
        Ok(path)
    }
}

/// Programmatic Akashic lint gate
pub fn validate_vault(vault_path: &Path) -> Result<LintReport, CrystallizerError> {
    let graph = VaultGraph::from_dir(vault_path)
        .map_err(|e| CrystallizerError::Failed(format!("Failed to build VaultGraph: {}", e)))?;
    let report = lint_vault(&graph);
    Ok(report)
}

pub struct Crystallizer;

impl Crystallizer {
    pub fn build_incident_node(
        id: impl Into<String>,
        title: impl Into<String>,
        scope: Scope,
    ) -> Node {
        Node {
            id: id.into(),
            node_type: "incident".to_string(),
            title: title.into(),
            status: Some("crystallized".to_string()),
            valid_from: None,
            valid_until: None,
            scope,
            relations: Vec::new(),
            path: None,
            body: None,
        }
    }

    pub fn link_cause(source: &mut Node, target_cause_id: impl Into<String>) {
        source.relations.push(Relation {
            relation_type: RelationType::CausedBy,
            target: target_cause_id.into(),
            evidence: None,
            scope: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    struct TempDir(PathBuf);
    impl TempDir {
        fn new() -> Self {
            use std::sync::atomic::{AtomicUsize, Ordering};
            static COUNTER: AtomicUsize = AtomicUsize::new(0);
            let count = COUNTER.fetch_add(1, Ordering::SeqCst);
            let path = std::env::temp_dir().join(format!(
                "crystallizer_test_{}_{}_{}",
                std::process::id(),
                count,
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir_all(&path).unwrap();
            TempDir(path)
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn test_forward_only_validation_rejects_resolved_by() {
        let node = IncidentNode {
            id: "INC-MOTOCAM-1001".to_string(),
            title: "Test Incident".to_string(),
            status: "active".to_string(),
            valid_from: Some("2026-10-19".to_string()),
            crash_signature: "abc123sig".to_string(),
            scope: BTreeMap::new(),
            external_refs: None,
            relations: vec![CrystallizerRelation {
                relation_type: "resolved_by".to_string(),
                target: "../resolutions/RES-MOTOCAM-1001-01.md".to_string(),
                evidence: None,
            }],
            body: "# Body".to_string(),
        };

        let err = node.validate_forward_only().unwrap_err();
        assert!(matches!(err, CrystallizerError::ForwardOnlyViolation(_)));
    }

    #[test]
    fn test_forward_only_validation_rejects_inverse_relations() {
        let relations = vec![
            CrystallizerRelation {
                relation_type: "superseded_by".to_string(),
                target: "target.md".to_string(),
                evidence: None,
            },
        ];
        let err = validate_relations(&relations).unwrap_err();
        assert!(matches!(err, CrystallizerError::ForwardOnlyViolation(_)));
    }

    #[test]
    fn test_vault_note_generation_and_lint_cleanliness() {
        let temp = TempDir::new();
        let writer = CrystallizerWriter::new(temp.path());

        let mut scope = BTreeMap::new();
        scope.insert("app".to_string(), "com.motorola.camera3".to_string());
        scope.insert("device".to_string(), "falcon".to_string());

        let incident = IncidentNode {
            id: "INC-MOTOCAM-9102".to_string(),
            title: "Camera preview pipeline null pointer dereference".to_string(),
            status: "active".to_string(),
            valid_from: Some("2026-10-19".to_string()),
            crash_signature: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_string(),
            scope: scope.clone(),
            external_refs: Some(ExternalRefs {
                jira_key: Some("MOTOCAM-9102".to_string()),
                bug2go_id: Some("B2G-882194".to_string()),
            }),
            relations: vec![CrystallizerRelation {
                relation_type: "supported_by".to_string(),
                target: "../evidence/EVD-MOTOCAM-9102-STACK.md".to_string(),
                evidence: None,
            }],
            body: "### Root Cause\nNull pointer when invoking notifyError without initialized buffer.".to_string(),
        };

        let evidence = EvidenceNode {
            id: "EVD-MOTOCAM-9102-STACK".to_string(),
            title: "Symbolicated stack trace for MOTOCAM-9102".to_string(),
            status: "active".to_string(),
            scope: scope.clone(),
            external_refs: Some(ExternalRefs {
                jira_key: Some("MOTOCAM-9102".to_string()),
                bug2go_id: Some("B2G-882194".to_string()),
            }),
            relations: vec![CrystallizerRelation {
                relation_type: "substantiates".to_string(),
                target: "../incidents/INC-MOTOCAM-9102.md".to_string(),
                evidence: None,
            }],
            body: "```\nat com.motorola.camera3.CameraDeviceClient.notifyError(CameraDeviceClient.java:412)\n```".to_string(),
        };

        let resolution = ResolutionNode {
            id: "RES-MOTOCAM-9102-01".to_string(),
            title: "Guard camera client state before notifyError".to_string(),
            status: "active".to_string(),
            remediation_type: "permanent_fix".to_string(),
            abandoned_reason: None,
            external_refs: Some(ResolutionExternalRefs {
                jira_key: Some("MOTOCAM-9102".to_string()),
                gerrit_cls: vec![GerritClRef {
                    repo: "platform/packages/apps/Camera3".to_string(),
                    change_id: "I123456789abcdef".to_string(),
                    cl_number: 44021,
                }],
            }),
            relations: vec![CrystallizerRelation {
                relation_type: "resolves".to_string(),
                target: "../incidents/INC-MOTOCAM-9102.md".to_string(),
                evidence: None,
            }],
            body: "### Fix Details\nAdds null check prior to dispatching error event.".to_string(),
        };

        writer.write_incident(&incident).unwrap();
        writer.write_evidence(&evidence).unwrap();
        writer.write_resolution(&resolution).unwrap();

        // Run programmatic akashic lint gate
        let report = validate_vault(temp.path()).unwrap();
        assert!(
            report.healthy,
            "Lint report should be healthy. Errors: {:?}",
            report.findings
        );
        assert_eq!(report.errors, 0);
        assert_eq!(report.total_nodes, 3);
    }
}
