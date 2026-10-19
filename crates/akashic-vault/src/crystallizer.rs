use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::frontmatter::{
    Frontmatter, NodeStatus, NodeType, Relation, RelationType, RemediationType, Scope,
};
use crate::parser::serialize_document;
use crate::VaultError;

/// Input payload for crystallizing an incident node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncidentInput {
    pub id: String,
    pub title: String,
    pub crash_signature: String,
    #[serde(default)]
    pub status: Option<NodeStatus>,
    #[serde(default)]
    pub scope: Scope,
    #[serde(default)]
    pub external_refs: Option<serde_yaml::Value>,
    #[serde(default)]
    pub relations: Vec<Relation>,
    pub body: String,
}

/// Input payload for crystallizing an evidence node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceInput {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub incident_id: Option<String>,
    #[serde(default)]
    pub status: Option<NodeStatus>,
    #[serde(default)]
    pub scope: Scope,
    #[serde(default)]
    pub invalidated_hypotheses: Vec<String>,
    #[serde(default)]
    pub failed_attempts: Vec<String>,
    #[serde(default)]
    pub relations: Vec<Relation>,
    pub body: String,
}

/// Input payload for crystallizing a resolution node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolutionInput {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub incident_id: Option<String>,
    pub remediation_type: RemediationType,
    #[serde(default)]
    pub status: Option<NodeStatus>,
    #[serde(default)]
    pub abandoned_reason: Option<String>,
    #[serde(default)]
    pub scope: Scope,
    #[serde(default)]
    pub external_refs: Option<serde_yaml::Value>,
    #[serde(default)]
    pub relations: Vec<Relation>,
    pub body: String,
}

/// A fully prepared RCA artifact ready to be written to disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrystallizedArtifact {
    pub id: String,
    pub frontmatter: Frontmatter,
    pub body: String,
    pub rendered_markdown: String,
}

/// Appends a structured `## Negative Knowledge` section if not already present.
fn maybe_append_negative_knowledge(
    body: &str,
    status: NodeStatus,
    abandoned_reason: Option<&str>,
    invalidated_hypotheses: &[String],
    failed_attempts: &[String],
) -> String {
    let has_negative_knowledge = status == NodeStatus::Abandoned
        || status == NodeStatus::Inconclusive
        || abandoned_reason.is_some()
        || !invalidated_hypotheses.is_empty()
        || !failed_attempts.is_empty();

    if !has_negative_knowledge || body.contains("## Negative Knowledge") {
        return body.to_string();
    }

    let mut section = String::from("\n\n## Negative Knowledge\n");

    if let Some(reason) = abandoned_reason {
        section.push_str(&format!("\n### Abandoned Rationale\n{}\n", reason.trim()));
    }

    if !invalidated_hypotheses.is_empty() {
        section.push_str("\n### Invalidated Hypotheses\n");
        for hyp in invalidated_hypotheses {
            section.push_str(&format!("- {}\n", hyp));
        }
    }

    if !failed_attempts.is_empty() {
        section.push_str("\n### Failed Attempts\n");
        for att in failed_attempts {
            section.push_str(&format!("- {}\n", att));
        }
    }

    if abandoned_reason.is_none() && invalidated_hypotheses.is_empty() && failed_attempts.is_empty() {
        section.push_str("\nThis investigation branch was abandoned or inconclusive.\n");
    }

    format!("{}{}", body.trim(), section)
}

/// Builds an incident artifact (`INC-*`).
pub fn build_incident_node(input: IncidentInput) -> Result<CrystallizedArtifact, VaultError> {
    let status = input.status.unwrap_or(NodeStatus::Active);

    let frontmatter = Frontmatter {
        id: input.id.clone(),
        title: input.title,
        node_type: NodeType::Incident,
        status,
        crash_signature: Some(input.crash_signature),
        scope: input.scope,
        external_refs: input.external_refs,
        relations: input.relations,
        ..Default::default()
    };

    let rendered = serialize_document(&frontmatter, &input.body)?;

    Ok(CrystallizedArtifact {
        id: input.id,
        frontmatter,
        body: input.body,
        rendered_markdown: rendered,
    })
}

/// Builds an evidence artifact (`EVD-*`).
pub fn build_evidence_node(input: EvidenceInput) -> Result<CrystallizedArtifact, VaultError> {
    let status = input.status.unwrap_or(NodeStatus::Active);
    let mut relations = input.relations;

    if let Some(ref inc_id) = input.incident_id {
        let exists = relations.iter().any(|r| {
            r.target == *inc_id && r.relation_type == RelationType::Substantiates
        });
        if !exists {
            relations.push(Relation {
                target: inc_id.clone(),
                relation_type: RelationType::Substantiates,
                evidence: None,
                scope: input.scope.clone(),
            });
        }
    }

    let final_body = maybe_append_negative_knowledge(
        &input.body,
        status,
        None,
        &input.invalidated_hypotheses,
        &input.failed_attempts,
    );

    let frontmatter = Frontmatter {
        id: input.id.clone(),
        title: input.title,
        node_type: NodeType::Evidence,
        status,
        scope: input.scope,
        invalidated_hypotheses: input.invalidated_hypotheses,
        failed_attempts: input.failed_attempts,
        relations,
        ..Default::default()
    };

    let rendered = serialize_document(&frontmatter, &final_body)?;

    Ok(CrystallizedArtifact {
        id: input.id,
        frontmatter,
        body: final_body,
        rendered_markdown: rendered,
    })
}

/// Builds a resolution artifact (`RES-*`).
pub fn build_resolution_node(input: ResolutionInput) -> Result<CrystallizedArtifact, VaultError> {
    let status = input.status.unwrap_or(NodeStatus::Resolved);
    let mut relations = input.relations;

    if let Some(ref inc_id) = input.incident_id {
        let exists = relations.iter().any(|r| {
            r.target == *inc_id && r.relation_type == RelationType::Resolves
        });
        if !exists {
            relations.push(Relation {
                target: inc_id.clone(),
                relation_type: RelationType::Resolves,
                evidence: None,
                scope: input.scope.clone(),
            });
        }
    }

    let final_body = maybe_append_negative_knowledge(
        &input.body,
        status,
        input.abandoned_reason.as_deref(),
        &[],
        &[],
    );

    let frontmatter = Frontmatter {
        id: input.id.clone(),
        title: input.title,
        node_type: NodeType::Resolution,
        status,
        remediation_type: Some(input.remediation_type),
        abandoned_reason: input.abandoned_reason,
        scope: input.scope,
        external_refs: input.external_refs,
        relations,
        ..Default::default()
    };

    let rendered = serialize_document(&frontmatter, &final_body)?;

    Ok(CrystallizedArtifact {
        id: input.id,
        frontmatter,
        body: final_body,
        rendered_markdown: rendered,
    })
}

/// Writes a crystallized artifact to `<vault_root>/<subdir>/<id>.md`.
pub fn write_vault_node(
    vault_root: &Path,
    subdir: &str,
    artifact: &CrystallizedArtifact,
) -> Result<PathBuf, VaultError> {
    let target_dir = vault_root.join(subdir);
    fs::create_dir_all(&target_dir)?;

    let file_path = target_dir.join(format!("{}.md", artifact.id));
    fs::write(&file_path, &artifact.rendered_markdown)?;

    Ok(file_path)
}
