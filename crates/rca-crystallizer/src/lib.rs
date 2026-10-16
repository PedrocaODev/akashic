use akashic::model::{Node, Relation, RelationType, Scope};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CrystallizerError {
    #[error("Crystallization failed: {0}")]
    Failed(String),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Invalid node structure: {0}")]
    InvalidNode(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrystallizedArtifact {
    pub root_node: Node,
    pub auxiliary_relations: Vec<Relation>,
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
