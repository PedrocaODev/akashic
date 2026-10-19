use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;

/// Scope bounds where a node or relation applies (e.g. env=production, system=payments).
pub type Scope = BTreeMap<String, String>;

/// Canonical typed node types supported by Google OKF v0.2.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum NodeType {
    Concept,
    Decision,
    Architecture,
    Incident,
    Evidence,
    Resolution,
    Runbook,
    Guide,
    Custom(String),
}

impl NodeType {
    pub fn as_str(&self) -> &str {
        match self {
            NodeType::Concept => "concept",
            NodeType::Decision => "decision",
            NodeType::Architecture => "architecture",
            NodeType::Incident => "incident",
            NodeType::Evidence => "evidence",
            NodeType::Resolution => "resolution",
            NodeType::Runbook => "runbook",
            NodeType::Guide => "guide",
            NodeType::Custom(s) => s.as_str(),
        }
    }
}

impl Default for NodeType {
    fn default() -> Self {
        NodeType::Concept
    }
}

impl fmt::Display for NodeType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl Serialize for NodeType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for NodeType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct NodeTypeVisitor;

        impl<'de> Visitor<'de> for NodeTypeVisitor {
            type Value = NodeType;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a node type string")
            }

            fn visit_str<E>(self, value: &str) -> Result<NodeType, E>
            where
                E: de::Error,
            {
                let trimmed = value.trim();
                let lower = trimmed.to_ascii_lowercase();
                Ok(match lower.as_str() {
                    "concept" => NodeType::Concept,
                    "decision" => NodeType::Decision,
                    "architecture" => NodeType::Architecture,
                    "incident" => NodeType::Incident,
                    "evidence" => NodeType::Evidence,
                    "resolution" => NodeType::Resolution,
                    "runbook" => NodeType::Runbook,
                    "guide" => NodeType::Guide,
                    _ => NodeType::Custom(trimmed.to_string()),
                })
            }
        }

        deserializer.deserialize_str(NodeTypeVisitor)
    }
}

/// Node lifecycle status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NodeStatus {
    Active,
    Current,
    Draft,
    Proposed,
    Superseded,
    Resolved,
    Abandoned,
    Inconclusive,
    Archived,
    Deprecated,
    Stable,
}

impl NodeStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            NodeStatus::Active => "active",
            NodeStatus::Current => "current",
            NodeStatus::Draft => "draft",
            NodeStatus::Proposed => "proposed",
            NodeStatus::Superseded => "superseded",
            NodeStatus::Resolved => "resolved",
            NodeStatus::Abandoned => "abandoned",
            NodeStatus::Inconclusive => "inconclusive",
            NodeStatus::Archived => "archived",
            NodeStatus::Deprecated => "deprecated",
            NodeStatus::Stable => "stable",
        }
    }

    pub fn is_active(&self) -> bool {
        matches!(
            self,
            NodeStatus::Active | NodeStatus::Current | NodeStatus::Resolved | NodeStatus::Stable
        )
    }
}

impl Default for NodeStatus {
    fn default() -> Self {
        NodeStatus::Draft
    }
}

impl fmt::Display for NodeStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl Serialize for NodeStatus {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for NodeStatus {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct NodeStatusVisitor;

        impl<'de> Visitor<'de> for NodeStatusVisitor {
            type Value = NodeStatus;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a node status string")
            }

            fn visit_str<E>(self, value: &str) -> Result<NodeStatus, E>
            where
                E: de::Error,
            {
                let trimmed = value.trim();
                let lower = trimmed.to_ascii_lowercase();
                match lower.as_str() {
                    "active" => Ok(NodeStatus::Active),
                    "current" => Ok(NodeStatus::Current),
                    "draft" => Ok(NodeStatus::Draft),
                    "proposed" => Ok(NodeStatus::Proposed),
                    "superseded" => Ok(NodeStatus::Superseded),
                    "resolved" => Ok(NodeStatus::Resolved),
                    "abandoned" => Ok(NodeStatus::Abandoned),
                    "inconclusive" => Ok(NodeStatus::Inconclusive),
                    "archived" => Ok(NodeStatus::Archived),
                    "deprecated" => Ok(NodeStatus::Deprecated),
                    "stable" => Ok(NodeStatus::Stable),
                    _ => Err(de::Error::unknown_variant(
                        trimmed,
                        &[
                            "active",
                            "current",
                            "draft",
                            "proposed",
                            "superseded",
                            "resolved",
                            "abandoned",
                            "inconclusive",
                            "archived",
                            "deprecated",
                            "stable",
                        ],
                    )),
                }
            }
        }

        deserializer.deserialize_str(NodeStatusVisitor)
    }
}

/// RCA resolution remediation type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RemediationType {
    Workaround,
    Hotfix,
    PermanentFix,
    Revert,
}

impl RemediationType {
    pub fn as_str(&self) -> &'static str {
        match self {
            RemediationType::Workaround => "workaround",
            RemediationType::Hotfix => "hotfix",
            RemediationType::PermanentFix => "permanent_fix",
            RemediationType::Revert => "revert",
        }
    }
}

impl fmt::Display for RemediationType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl Serialize for RemediationType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for RemediationType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct RemediationTypeVisitor;

        impl<'de> Visitor<'de> for RemediationTypeVisitor {
            type Value = RemediationType;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a remediation type string")
            }

            fn visit_str<E>(self, value: &str) -> Result<RemediationType, E>
            where
                E: de::Error,
            {
                let trimmed = value.trim();
                let lower = trimmed.to_ascii_lowercase().replace('-', "_");
                match lower.as_str() {
                    "workaround" => Ok(RemediationType::Workaround),
                    "hotfix" => Ok(RemediationType::Hotfix),
                    "permanent_fix" | "permanentfix" => Ok(RemediationType::PermanentFix),
                    "revert" => Ok(RemediationType::Revert),
                    _ => Err(de::Error::unknown_variant(
                        trimmed,
                        &["workaround", "hotfix", "permanent_fix", "revert"],
                    )),
                }
            }
        }

        deserializer.deserialize_str(RemediationTypeVisitor)
    }
}

/// Forward-only relation types compliant with OKF v0.2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RelationType {
    Supersedes,
    DependsOn,
    SupportedBy,
    CausedBy,
    RelatesTo,
    Contradicts,
    Substantiates,
    Resolves,
}

impl RelationType {
    pub fn as_str(&self) -> &'static str {
        match self {
            RelationType::Supersedes => "supersedes",
            RelationType::DependsOn => "depends_on",
            RelationType::SupportedBy => "supported_by",
            RelationType::CausedBy => "caused_by",
            RelationType::RelatesTo => "relates_to",
            RelationType::Contradicts => "contradicts",
            RelationType::Substantiates => "substantiates",
            RelationType::Resolves => "resolves",
        }
    }

    pub fn from_str_loose(s: &str) -> Option<Self> {
        let trimmed = s.trim();
        let lower = trimmed.to_ascii_lowercase().replace('-', "_");
        match lower.as_str() {
            "supersedes" => Some(RelationType::Supersedes),
            "depends_on" | "dependson" => Some(RelationType::DependsOn),
            "supported_by" | "supportedby" => Some(RelationType::SupportedBy),
            "caused_by" | "causedby" => Some(RelationType::CausedBy),
            "relates_to" | "relatesto" => Some(RelationType::RelatesTo),
            "contradicts" => Some(RelationType::Contradicts),
            "substantiates" => Some(RelationType::Substantiates),
            "resolves" => Some(RelationType::Resolves),
            _ => None,
        }
    }
}

impl fmt::Display for RelationType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl Serialize for RelationType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for RelationType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct RelationTypeVisitor;

        impl<'de> Visitor<'de> for RelationTypeVisitor {
            type Value = RelationType;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a forward-only relation type")
            }

            fn visit_str<E>(self, value: &str) -> Result<RelationType, E>
            where
                E: de::Error,
            {
                RelationType::from_str_loose(value).ok_or_else(|| {
                    de::Error::unknown_variant(
                        value,
                        &[
                            "supersedes",
                            "depends_on",
                            "supported_by",
                            "caused_by",
                            "relates_to",
                            "contradicts",
                            "substantiates",
                            "resolves",
                        ],
                    )
                })
            }
        }

        deserializer.deserialize_str(RelationTypeVisitor)
    }
}

/// Prohibited reverse relations that violate the OKF v0.2 forward-only rule.
pub const PROHIBITED_REVERSE_RELATIONS: &[&str] = &[
    "superseded_by",
    "resolved_by",
    "depended_on_by",
    "required_by",
    "supports",
    "caused",
];

/// Checks whether a relation name is a prohibited reverse relation.
pub fn is_prohibited_reverse_relation(s: &str) -> bool {
    let normalized = s.trim().to_ascii_lowercase().replace('-', "_");
    PROHIBITED_REVERSE_RELATIONS
        .iter()
        .any(|&p| p == normalized || p.replace('_', "") == normalized.replace('_', ""))
}

/// A typed, directed edge connecting a source node to a target node.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Relation {
    pub target: String,
    #[serde(rename = "type")]
    pub relation_type: RelationType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub scope: Scope,
}

/// Complete frontmatter definition for an OKF v0.2 vault node.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Frontmatter {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(rename = "type", default)]
    pub node_type: NodeType,
    #[serde(default)]
    pub status: NodeStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_from: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_until: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub scope: Scope,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub owners: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relations: Vec<Relation>,

    // First-class RCA fields
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crash_signature: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remediation_type: Option<RemediationType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub abandoned_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub invalidated_hypotheses: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub failed_attempts: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_refs: Option<serde_yaml::Value>,

    // Arbitrary user-defined fields preserved losslessly
    #[serde(flatten)]
    pub custom: BTreeMap<String, serde_yaml::Value>,
}

impl Default for Frontmatter {
    fn default() -> Self {
        Self {
            id: String::new(),
            title: String::new(),
            node_type: NodeType::Concept,
            status: NodeStatus::Draft,
            valid_from: None,
            valid_until: None,
            confidence: None,
            scope: BTreeMap::new(),
            tags: Vec::new(),
            owners: Vec::new(),
            sources: Vec::new(),
            relations: Vec::new(),
            crash_signature: None,
            remediation_type: None,
            abandoned_reason: None,
            invalidated_hypotheses: Vec::new(),
            failed_attempts: Vec::new(),
            external_refs: None,
            custom: BTreeMap::new(),
        }
    }
}

pub type VaultNodeFrontmatter = Frontmatter;
pub type NodeMetadata = Frontmatter;

/// Extracted markdown or wikilink reference from document body.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RawLink {
    pub target: String,
    pub text: String,
    pub is_wikilink: bool,
}

/// A parsed document from disk with extracted metadata and body.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ParsedDocument {
    pub path: PathBuf,
    pub relative_path: String,
    pub frontmatter: Frontmatter,
    pub raw_yaml: Option<String>,
    pub body: String,
    pub raw_links: Vec<RawLink>,
    pub missing_required_fields: Vec<String>,
    pub prohibited_relations: Vec<String>,
}

/// Query mode selector for vault graph queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryMode {
    Current,
    Lineage,
    Impact,
    Historical,
}

impl fmt::Display for QueryMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            QueryMode::Current => write!(f, "current"),
            QueryMode::Lineage => write!(f, "lineage"),
            QueryMode::Impact => write!(f, "impact"),
            QueryMode::Historical => write!(f, "historical"),
        }
    }
}

/// Conflict alert generated when graph queries encounter ambiguity.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConflictAlert {
    pub alert: String,
    pub kind: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub predecessor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cycle: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub candidates: Vec<String>,
}

/// Summary of a node suitable for inclusion in ContextPacket.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeSummary {
    pub id: String,
    pub title: String,
    pub node_type: NodeType,
    pub status: NodeStatus,
    pub path: Option<String>,
    pub scope: Scope,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub custom: BTreeMap<String, serde_yaml::Value>,
}

impl From<&Frontmatter> for NodeSummary {
    fn from(fm: &Frontmatter) -> Self {
        Self {
            id: fm.id.clone(),
            title: fm.title.clone(),
            node_type: fm.node_type.clone(),
            status: fm.status,
            path: None,
            scope: fm.scope.clone(),
            confidence: fm.confidence,
            tags: fm.tags.clone(),
            custom: fm.custom.clone(),
        }
    }
}

impl From<&ParsedDocument> for NodeSummary {
    fn from(doc: &ParsedDocument) -> Self {
        Self {
            id: doc.frontmatter.id.clone(),
            title: doc.frontmatter.title.clone(),
            node_type: doc.frontmatter.node_type.clone(),
            status: doc.frontmatter.status,
            path: Some(doc.relative_path.clone()),
            scope: doc.frontmatter.scope.clone(),
            confidence: doc.frontmatter.confidence,
            tags: doc.frontmatter.tags.clone(),
            custom: doc.frontmatter.custom.clone(),
        }
    }
}

/// Edge summary connecting two nodes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EdgeSummary {
    pub source: String,
    pub target: String,
    pub relation_type: RelationType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub scope: Scope,
}

/// A step in a supersession chain.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupersessionStep {
    pub predecessor: String,
    pub successor: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub scope: Scope,
}

/// Predecessor retained for historical rationale.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HistoricalRationaleNote {
    pub summary: NodeSummary,
    pub superseded_by: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub scope: Scope,
}

/// An entry in a lineage trace chain.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LineageEntry {
    pub node: NodeSummary,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supersedes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub superseded_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<String>,
    pub is_current: bool,
}

/// An item affected in an impact analysis.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ImpactItem {
    pub id: String,
    pub title: String,
    pub node_type: NodeType,
    pub depth: usize,
    pub path: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub scope: Scope,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
}

/// Result packet for current guidance mode.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CurrentGuidancePacket {
    pub mode: String,
    pub seed: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_guidance_id: Option<String>,
    pub is_current: bool,
    pub status: String,
    pub query_scope: Scope,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_node: Option<NodeSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unresolved_conflict: Option<ConflictAlert>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub supersession_chain: Vec<SupersessionStep>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub historical_rationale: Vec<HistoricalRationaleNote>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub advisories: Vec<String>,
    pub message: String,
}

/// Result packet for lineage query mode.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LineagePacket {
    pub mode: String,
    pub seed: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_guidance_id: Option<String>,
    pub query_scope: Scope,
    pub chain: Vec<LineageEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub transitions: Vec<SupersessionStep>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub empirical_evidence: Vec<NodeSummary>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub advisories: Vec<String>,
    pub message: String,
}

/// Result packet for impact analysis mode.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ImpactPacket {
    pub mode: String,
    pub seed: String,
    pub seed_title: String,
    pub query_scope: Scope,
    pub total_dependents: usize,
    pub max_depth: usize,
    pub dependents: Vec<ImpactItem>,
    pub message: String,
}

/// Result packet for historical point-in-time mode.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HistoricalPacket {
    pub mode: String,
    pub seed: String,
    pub as_of: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_guidance_id: Option<String>,
    pub is_active: bool,
    pub status: String,
    pub query_scope: Scope,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_node: Option<NodeSummary>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub advisories: Vec<String>,
    pub message: String,
}

/// Universal context packet returned by graph query operations.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContextPacket {
    pub mode: QueryMode,
    pub seed_id: String,
    pub scope_filter: Scope,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub as_of: Option<String>,
    pub abstained: bool,
    pub focal_nodes: Vec<NodeSummary>,
    pub ancestors: Vec<NodeSummary>,
    pub descendants: Vec<NodeSummary>,
    pub supporting_evidence: Vec<NodeSummary>,
    pub contradictions: Vec<NodeSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unresolved_conflict: Option<ConflictAlert>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unresolved_conflicts: Vec<ConflictAlert>,
    pub edges: Vec<EdgeSummary>,
    pub warnings: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_packet: Option<CurrentGuidancePacket>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lineage_packet: Option<LineagePacket>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub impact_packet: Option<ImpactPacket>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub historical_packet: Option<HistoricalPacket>,
}

/// Checks whether two scopes overlap (i.e. do not contradict on any shared key).
pub fn scopes_overlap(a: &Scope, b: &Scope) -> bool {
    for (k, va) in a {
        if let Some(vb) = b.get(k) {
            if va != vb {
                return false;
            }
        }
    }
    true
}

/// Checks whether scope `sup` covers scope `sub` (all constraints in `sup` match in `sub`).
pub fn scope_covers(sup: &Scope, sub: &Scope) -> bool {
    sup.iter().all(|(k, v)| sub.get(k) == Some(v))
}
