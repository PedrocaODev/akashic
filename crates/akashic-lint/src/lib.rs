use akashic_vault::{
    scopes_overlap, serde_yaml, NodeStatus, NodeType, ParsedDocument, RelationType, VaultConfig,
    VaultGraph,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LintError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Parse error: {0}")]
    Parse(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Severity::Error => write!(f, "error"),
            Severity::Warning => write!(f, "warning"),
            Severity::Info => write!(f, "info"),
        }
    }
}

fn default_field_type() -> String {
    "string".to_string()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomFieldRule {
    #[serde(default)]
    pub required: bool,
    #[serde(rename = "type", alias = "field_type", default = "default_field_type")]
    pub field_type: String,
    #[serde(default)]
    pub allowed_values: Vec<String>,
    #[serde(default)]
    pub applies_to: Vec<String>,
}

impl Default for CustomFieldRule {
    fn default() -> Self {
        Self {
            required: false,
            field_type: default_field_type(),
            allowed_values: Vec::new(),
            applies_to: Vec::new(),
        }
    }
}

pub fn parse_custom_field_rules_from_toml(
    toml_str: &str,
) -> Result<BTreeMap<String, CustomFieldRule>, LintError> {
    if toml_str.trim().is_empty() {
        return Ok(BTreeMap::new());
    }

    let val: toml::Value =
        toml::from_str(toml_str).map_err(|e| LintError::Parse(e.to_string()))?;

    // Check [lint.custom_fields]
    if let Some(lint) = val.get("lint") {
        if let Some(cf) = lint.get("custom_fields") {
            let rules: BTreeMap<String, CustomFieldRule> =
                cf.clone().try_into().map_err(|e: toml::de::Error| LintError::Parse(e.to_string()))?;
            return Ok(rules);
        }
    }

    // Check [custom_fields]
    if let Some(cf) = val.get("custom_fields") {
        let rules: BTreeMap<String, CustomFieldRule> =
            cf.clone().try_into().map_err(|e: toml::de::Error| LintError::Parse(e.to_string()))?;
        return Ok(rules);
    }

    // Direct table deserialize
    if let Ok(rules) = val.try_into::<BTreeMap<String, CustomFieldRule>>() {
        return Ok(rules);
    }

    Ok(BTreeMap::new())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LintIssue {
    pub code: String,
    pub severity: Severity,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remediation: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LintReport {
    pub valid: bool,
    pub error_count: usize,
    pub warning_count: usize,
    pub info_count: usize,
    pub issues: Vec<LintIssue>,
}

impl LintReport {
    pub fn new(issues: Vec<LintIssue>) -> Self {
        let error_count = issues.iter().filter(|i| i.severity == Severity::Error).count();
        let warning_count = issues.iter().filter(|i| i.severity == Severity::Warning).count();
        let info_count = issues.iter().filter(|i| i.severity == Severity::Info).count();
        let valid = error_count == 0;
        Self {
            valid,
            error_count,
            warning_count,
            info_count,
            issues,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LintConfig {
    pub enabled_rules: Vec<String>,
    pub strict_forward_only: bool,
    #[serde(default)]
    pub custom_fields: HashMap<String, String>,
    #[serde(default)]
    pub custom_field_rules: BTreeMap<String, CustomFieldRule>,
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
                "L012".into(),
            ],
            strict_forward_only: true,
            custom_fields: HashMap::new(),
            custom_field_rules: BTreeMap::new(),
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

    pub fn lint_graph(&self, graph: &VaultGraph) -> LintReport {
        lint_vault_with_config(graph, &self.config)
    }

    pub fn lint_vault_dir(&self, root: &Path) -> Result<LintReport, LintError> {
        let graph = VaultGraph::from_dir(root).map_err(|e| LintError::Parse(e.to_string()))?;
        Ok(self.lint_graph(&graph))
    }
}

pub trait AsLintConfig {
    fn to_lint_config(&self) -> LintConfig;
}

impl AsLintConfig for LintConfig {
    fn to_lint_config(&self) -> LintConfig {
        self.clone()
    }
}

impl AsLintConfig for BTreeMap<String, CustomFieldRule> {
    fn to_lint_config(&self) -> LintConfig {
        let mut cfg = LintConfig::default();
        cfg.custom_field_rules = self.clone();
        cfg
    }
}

impl<T: AsLintConfig + ?Sized> AsLintConfig for &T {
    fn to_lint_config(&self) -> LintConfig {
        (*self).to_lint_config()
    }
}

pub fn lint_vault(graph: &VaultGraph) -> LintReport {
    let config = LintConfig::default();
    lint_vault_with_config(graph, &config)
}

pub fn lint_vault_with_rules(
    graph: &VaultGraph,
    custom_rules: &BTreeMap<String, CustomFieldRule>,
) -> LintReport {
    let mut config = LintConfig::default();
    config.custom_field_rules = custom_rules.clone();
    lint_vault_with_config(graph, &config)
}

pub fn lint_vault_with_config<C: AsLintConfig>(graph: &VaultGraph, config: C) -> LintReport {
    let config = config.to_lint_config();
    let mut issues = Vec::new();

    // L001: Missing or invalid YAML frontmatter
    if config.enabled_rules.contains(&"L001".to_string()) {
        for (path, err) in &graph.malformed_files {
            issues.push(LintIssue {
                code: "L001".to_string(),
                severity: Severity::Error,
                message: format!("Malformed file '{}': {}", path.display(), err),
                node_id: None,
                path: Some(path.to_string_lossy().to_string()),
                remediation: Some(
                    "Ensure file starts and ends with '---' frontmatter delimiters with valid YAML"
                        .to_string(),
                ),
            });
        }
    }

    // L002: Missing required fields (id, type, status)
    if config.enabled_rules.contains(&"L002".to_string()) {
        for doc in graph.graph.node_weights() {
            for field in &doc.missing_required_fields {
                issues.push(LintIssue {
                    code: "L002".to_string(),
                    severity: Severity::Error,
                    message: format!(
                        "Node '{}' is missing required frontmatter field '{}'",
                        doc.frontmatter.id, field
                    ),
                    node_id: Some(doc.frontmatter.id.clone()),
                    path: Some(doc.relative_path.clone()),
                    remediation: Some(format!("Add required field '{}' to frontmatter", field)),
                });
            }
        }
    }

    // L003: Duplicate Vault Node id
    if config.enabled_rules.contains(&"L003".to_string()) {
        for (id, p1, p2) in &graph.duplicate_ids {
            issues.push(LintIssue {
                code: "L003".to_string(),
                severity: Severity::Error,
                message: format!(
                    "Duplicate node ID '{}' found in '{}' and '{}'",
                    id,
                    p1.display(),
                    p2.display()
                ),
                node_id: Some(id.clone()),
                path: Some(p2.to_string_lossy().to_string()),
                remediation: Some(format!("Ensure node ID '{}' is unique across the vault", id)),
            });
        }
    }

    // L004: Broken Relation target or broken evidence target
    if config.enabled_rules.contains(&"L004".to_string()) {
        for unres in &graph.unresolved_relations {
            issues.push(LintIssue {
                code: "L004".to_string(),
                severity: Severity::Error,
                message: format!(
                    "Node '{}' has relation '{}' targeting '{}' which does not resolve to any node or file in the vault",
                    unres.source_id, unres.relation_type, unres.target
                ),
                node_id: Some(unres.source_id.clone()),
                path: Some(unres.source_path.clone()),
                remediation: Some(format!(
                    "Fix relation target '{}' to point to an existing node ID or relative path",
                    unres.target
                )),
            });
        }

        for doc in graph.graph.node_weights() {
            let src_dir = Path::new(&doc.relative_path)
                .parent()
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            let src_dir = if src_dir == "." { "" } else { &src_dir };

            for rel in &doc.frontmatter.relations {
                if let Some(ref ev) = rel.evidence {
                    let ev_clean = ev.trim();
                    if !ev_clean.is_empty()
                        && !ev_clean.starts_with("http://")
                        && !ev_clean.starts_with("https://")
                    {
                        let in_graph = graph.node_by_id.contains_key(ev_clean)
                            || graph.resolve_target(src_dir, ev_clean).is_some();
                        let on_disk = graph.root.join(ev_clean).exists()
                            || (src_dir.len() > 0
                                && graph.root.join(src_dir).join(ev_clean).exists())
                            || graph.root.join(format!("{}.md", ev_clean)).exists()
                            || (src_dir.len() > 0
                                && graph
                                    .root
                                    .join(src_dir)
                                    .join(format!("{}.md", ev_clean))
                                    .exists());

                        if !in_graph && !on_disk {
                            issues.push(LintIssue {
                                code: "L004".to_string(),
                                severity: Severity::Error,
                                message: format!(
                                    "Node '{}' relation targeting '{}' references missing evidence target '{}'",
                                    doc.frontmatter.id, rel.target, ev_clean
                                ),
                                node_id: Some(doc.frontmatter.id.clone()),
                                path: Some(doc.relative_path.clone()),
                                remediation: Some(format!(
                                    "Ensure evidence '{}' exists in the vault, on disk, or is an HTTP/HTTPS URL",
                                    ev_clean
                                )),
                            });
                        }
                    }
                }
            }
        }
    }

    // L005: Directed cycles in supersedes or depends_on
    if config.enabled_rules.contains(&"L005".to_string()) {
        let supersedes_cycles = graph.find_cycles(&[RelationType::Supersedes]);
        for cycle in supersedes_cycles {
            let cycle_str = if cycle.len() == 1 {
                format!("{} -> {}", cycle[0], cycle[0])
            } else {
                format!("{} -> {}", cycle.join(" -> "), cycle[0])
            };
            issues.push(LintIssue {
                code: "L005".to_string(),
                severity: Severity::Error,
                message: format!(
                    "Directed cycle detected in 'supersedes' relation: {}",
                    cycle_str
                ),
                node_id: cycle.first().cloned(),
                path: cycle
                    .first()
                    .and_then(|id| graph.get_node(id))
                    .map(|d| d.relative_path.clone()),
                remediation: Some(
                    "Break the circular 'supersedes' dependency to restore a directed acyclic graph"
                        .to_string(),
                ),
            });
        }

        let depends_cycles = graph.find_cycles(&[RelationType::DependsOn]);
        for cycle in depends_cycles {
            let cycle_str = if cycle.len() == 1 {
                format!("{} -> {}", cycle[0], cycle[0])
            } else {
                format!("{} -> {}", cycle.join(" -> "), cycle[0])
            };
            issues.push(LintIssue {
                code: "L005".to_string(),
                severity: Severity::Error,
                message: format!(
                    "Directed cycle detected in 'depends_on' relation: {}",
                    cycle_str
                ),
                node_id: cycle.first().cloned(),
                path: cycle
                    .first()
                    .and_then(|id| graph.get_node(id))
                    .map(|d| d.relative_path.clone()),
                remediation: Some(
                    "Break the circular 'depends_on' dependency to restore a directed acyclic graph"
                        .to_string(),
                ),
            });
        }
    }

    // L006: Node with status == NodeStatus::Superseded that has no incoming RelationType::Supersedes edge
    if config.enabled_rules.contains(&"L006".to_string()) {
        for doc in graph.graph.node_weights() {
            if doc.frontmatter.status == NodeStatus::Superseded {
                let incoming = graph.incoming_edges(&doc.frontmatter.id);
                let has_supersedes =
                    incoming.iter().any(|e| e.relation_type == RelationType::Supersedes);
                if !has_supersedes {
                    issues.push(LintIssue {
                        code: "L006".to_string(),
                        severity: Severity::Warning,
                        message: format!(
                            "Node '{}' has status 'superseded' but no successor node supersedes it",
                            doc.frontmatter.id
                        ),
                        node_id: Some(doc.frontmatter.id.clone()),
                        path: Some(doc.relative_path.clone()),
                        remediation: Some(format!(
                            "Add a successor node declaring 'supersedes: {}' or update node status",
                            doc.frontmatter.id
                        )),
                    });
                }
            }
        }
    }

    // L007: Competing active successors (Unresolved Conflict)
    if config.enabled_rules.contains(&"L007".to_string()) {
        let mut seen_conflicts = HashSet::new();

        for doc in graph.graph.node_weights() {
            let pred_id = &doc.frontmatter.id;
            let incoming = graph.incoming_edges(pred_id);

            let mut active_successors = Vec::new();
            for edge in &incoming {
                if edge.relation_type == RelationType::Supersedes {
                    if let Some(succ_doc) = graph.get_node(&edge.source) {
                        if succ_doc.frontmatter.status.is_active() {
                            let succ_scope = if !edge.scope.is_empty() {
                                &edge.scope
                            } else {
                                &succ_doc.frontmatter.scope
                            };
                            active_successors.push((succ_doc, succ_scope));
                        }
                    }
                }
            }

            if active_successors.len() > 1 {
                for i in 0..active_successors.len() {
                    for j in (i + 1)..active_successors.len() {
                        let (s1, scope1) = active_successors[i];
                        let (s2, scope2) = active_successors[j];

                        let s1_supersedes_s2 = graph
                            .outgoing_edges(&s1.frontmatter.id)
                            .iter()
                            .any(|e| {
                                e.relation_type == RelationType::Supersedes
                                    && e.target == s2.frontmatter.id
                            });
                        let s2_supersedes_s1 = graph
                            .outgoing_edges(&s2.frontmatter.id)
                            .iter()
                            .any(|e| {
                                e.relation_type == RelationType::Supersedes
                                    && e.target == s1.frontmatter.id
                            });

                        if !s1_supersedes_s2
                            && !s2_supersedes_s1
                            && scopes_overlap(scope1, scope2)
                        {
                            let mut pair = [s1.frontmatter.id.clone(), s2.frontmatter.id.clone()];
                            pair.sort();
                            if seen_conflicts
                                .insert((pred_id.clone(), pair[0].clone(), pair[1].clone()))
                            {
                                issues.push(LintIssue {
                                    code: "L007".to_string(),
                                    severity: Severity::Error,
                                    message: format!(
                                        "Competing active successors: '{}' and '{}' both supersede '{}' with overlapping scopes without superseding each other",
                                        pair[0], pair[1], pred_id
                                    ),
                                    node_id: Some(pred_id.clone()),
                                    path: Some(doc.relative_path.clone()),
                                    remediation: Some(format!(
                                        "Declare which successor supersedes the other between '{}' and '{}', or partition their scopes",
                                        pair[0], pair[1]
                                    )),
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    // L008: Unscoped contradiction
    if config.enabled_rules.contains(&"L008".to_string()) {
        let mut seen_contradictions = HashSet::new();

        for doc in graph.graph.node_weights() {
            for rel in &doc.frontmatter.relations {
                if rel.relation_type == RelationType::Contradicts {
                    let tgt_doc_opt = graph.get_node(&rel.target);
                    let tgt_scope_empty =
                        tgt_doc_opt.map_or(true, |t| t.frontmatter.scope.is_empty());

                    if rel.scope.is_empty()
                        && doc.frontmatter.scope.is_empty()
                        && tgt_scope_empty
                    {
                        let mut pair = [doc.frontmatter.id.clone(), rel.target.clone()];
                        pair.sort();
                        if seen_contradictions.insert((pair[0].clone(), pair[1].clone())) {
                            issues.push(LintIssue {
                                code: "L008".to_string(),
                                severity: Severity::Warning,
                                message: format!(
                                    "Unscoped contradiction between '{}' and '{}': neither the relation nor the nodes specify a scope",
                                    pair[0], pair[1]
                                ),
                                node_id: Some(doc.frontmatter.id.clone()),
                                path: Some(doc.relative_path.clone()),
                                remediation: Some(
                                    "Define scope constraints (e.g. env, system) to qualify the contradiction context"
                                        .to_string(),
                                ),
                            });
                        }
                    }
                }
            }
        }
    }

    // L009: Missing valid_from on current or active guidance
    if config.enabled_rules.contains(&"L009".to_string()) {
        for doc in graph.graph.node_weights() {
            if matches!(doc.frontmatter.status, NodeStatus::Current | NodeStatus::Active)
                && doc
                    .frontmatter
                    .valid_from
                    .as_deref()
                    .unwrap_or("")
                    .trim()
                    .is_empty()
            {
                issues.push(LintIssue {
                    code: "L009".to_string(),
                    severity: Severity::Warning,
                    message: format!(
                        "Node '{}' with status '{}' is missing required 'valid_from' timestamp",
                        doc.frontmatter.id, doc.frontmatter.status
                    ),
                    node_id: Some(doc.frontmatter.id.clone()),
                    path: Some(doc.relative_path.clone()),
                    remediation: Some(
                        "Specify 'valid_from: YYYY-MM-DD' in frontmatter to enable point-in-time validity tracking"
                            .to_string(),
                    ),
                });
            }
        }
    }

    // L010: High-impact relation missing evidence
    if config.enabled_rules.contains(&"L010".to_string()) {
        for doc in graph.graph.node_weights() {
            let has_outgoing_evidence =
                graph.outgoing_edges(&doc.frontmatter.id).iter().any(|e| {
                    (e.relation_type == RelationType::SupportedBy
                        || e.relation_type == RelationType::Substantiates)
                        && graph
                            .get_node(&e.target)
                            .map_or(false, |t| t.frontmatter.node_type == NodeType::Evidence)
                });

            for rel in &doc.frontmatter.relations {
                let is_high_impact = matches!(
                    rel.relation_type,
                    RelationType::Supersedes
                        | RelationType::Contradicts
                        | RelationType::CausedBy
                        | RelationType::Resolves
                );

                if is_high_impact && rel.evidence.as_deref().unwrap_or("").trim().is_empty() {
                    let target_is_evidence = graph
                        .get_node(&rel.target)
                        .map_or(false, |t| t.frontmatter.node_type == NodeType::Evidence);

                    if !target_is_evidence && !has_outgoing_evidence {
                        issues.push(LintIssue {
                            code: "L010".to_string(),
                            severity: Severity::Warning,
                            message: format!(
                                "High-impact relation '{}' from '{}' to '{}' lacks supporting evidence",
                                rel.relation_type, doc.frontmatter.id, rel.target
                            ),
                            node_id: Some(doc.frontmatter.id.clone()),
                            path: Some(doc.relative_path.clone()),
                            remediation: Some(
                                "Add an 'evidence' attribute or link to an Evidence node via 'supported_by'"
                                    .to_string(),
                            ),
                        });
                    }
                }
            }
        }
    }

    // L011: Prohibited reverse relation
    if config.enabled_rules.contains(&"L011".to_string()) {
        for doc in graph.graph.node_weights() {
            for proh in &doc.prohibited_relations {
                issues.push(LintIssue {
                    code: "L011".to_string(),
                    severity: Severity::Error,
                    message: format!(
                        "Node '{}' declares prohibited reverse relation '{}'",
                        doc.frontmatter.id, proh
                    ),
                    node_id: Some(doc.frontmatter.id.clone()),
                    path: Some(doc.relative_path.clone()),
                    remediation: Some(format!(
                        "Convert reverse relation '{}' to forward-only architecture (e.g. declare 'supersedes' in the successor)",
                        proh
                    )),
                });
            }
        }
    }

    // L012: [lint.custom_fields] validation
    if config.enabled_rules.contains(&"L012".to_string()) {
        let mut merged_rules = config.custom_field_rules.clone();
        for (k, v) in &config.custom_fields {
            merged_rules.entry(k.clone()).or_insert_with(|| CustomFieldRule {
                required: false,
                field_type: v.clone(),
                allowed_values: Vec::new(),
                applies_to: Vec::new(),
            });
        }

        for doc in graph.graph.node_weights() {
            for (field_name, rule) in &merged_rules {
                let applies = rule.applies_to.is_empty()
                    || rule.applies_to.iter().any(|t| {
                        t.eq_ignore_ascii_case(doc.frontmatter.node_type.as_str())
                    });

                if !applies {
                    continue;
                }

                let val_opt = doc.frontmatter.custom.get(field_name);
                let is_missing = val_opt.is_none() || val_opt == Some(&serde_yaml::Value::Null);

                if is_missing {
                    if rule.required {
                        issues.push(LintIssue {
                            code: "L012".to_string(),
                            severity: Severity::Error,
                            message: format!(
                                "Node '{}' is missing required custom field '{}'",
                                doc.frontmatter.id, field_name
                            ),
                            node_id: Some(doc.frontmatter.id.clone()),
                            path: Some(doc.relative_path.clone()),
                            remediation: Some(format!(
                                "Add custom field '{}' to frontmatter",
                                field_name
                            )),
                        });
                    }
                } else if let Some(val) = val_opt {
                    validate_custom_field(doc, field_name, val, rule, &mut issues);
                }
            }
        }
    }

    issues.sort_by(|a, b| {
        a.severity
            .cmp(&b.severity)
            .then_with(|| a.code.cmp(&b.code))
            .then_with(|| a.path.cmp(&b.path))
            .then_with(|| a.node_id.cmp(&b.node_id))
            .then_with(|| a.message.cmp(&b.message))
    });

    LintReport::new(issues)
}

fn validate_custom_field(
    doc: &ParsedDocument,
    field_name: &str,
    val: &serde_yaml::Value,
    rule: &CustomFieldRule,
    issues: &mut Vec<LintIssue>,
) {
    let norm_type = rule.field_type.trim().to_ascii_lowercase();
    let mut type_ok = true;

    match norm_type.as_str() {
        "string" => {
            if val.as_str().is_none() {
                type_ok = false;
            }
        }
        "integer" | "int" => {
            if !val.is_i64() && !val.is_u64() {
                type_ok = false;
            }
        }
        "number" | "float" => {
            if !val.is_number() {
                type_ok = false;
            }
        }
        "boolean" | "bool" => {
            if !val.is_bool() {
                type_ok = false;
            }
        }
        "list" | "array" => {
            if !val.is_sequence() {
                type_ok = false;
            }
        }
        "enum" => {
            if val.as_str().is_none() {
                type_ok = false;
            }
        }
        _ => {}
    }

    if !type_ok {
        issues.push(LintIssue {
            code: "L012".to_string(),
            severity: Severity::Error,
            message: format!(
                "Custom field '{}' in node '{}' must be of type '{}', found {:?}",
                field_name, doc.frontmatter.id, rule.field_type, val
            ),
            node_id: Some(doc.frontmatter.id.clone()),
            path: Some(doc.relative_path.clone()),
            remediation: Some(format!(
                "Ensure '{}' conforms to type '{}'",
                field_name, rule.field_type
            )),
        });
        return;
    }

    if !rule.allowed_values.is_empty() {
        if let Some(seq) = val.as_sequence() {
            for item in seq {
                let item_str = match item {
                    serde_yaml::Value::String(s) => s.clone(),
                    serde_yaml::Value::Number(n) => n.to_string(),
                    serde_yaml::Value::Bool(b) => b.to_string(),
                    _ => format!("{:?}", item),
                };
                if !rule.allowed_values.iter().any(|allowed| allowed == &item_str) {
                    issues.push(LintIssue {
                        code: "L012".to_string(),
                        severity: Severity::Error,
                        message: format!(
                            "Custom field '{}' in node '{}' has disallowed value '{}', expected one of {:?}",
                            field_name, doc.frontmatter.id, item_str, rule.allowed_values
                        ),
                        node_id: Some(doc.frontmatter.id.clone()),
                        path: Some(doc.relative_path.clone()),
                        remediation: Some(format!(
                            "Use one of the allowed values: {}",
                            rule.allowed_values.join(", ")
                        )),
                    });
                }
            }
        } else {
            let val_str = match val {
                serde_yaml::Value::String(s) => s.clone(),
                serde_yaml::Value::Number(n) => n.to_string(),
                serde_yaml::Value::Bool(b) => b.to_string(),
                _ => format!("{:?}", val),
            };
            if !rule.allowed_values.iter().any(|allowed| allowed == &val_str) {
                issues.push(LintIssue {
                    code: "L012".to_string(),
                    severity: Severity::Error,
                    message: format!(
                        "Custom field '{}' in node '{}' has disallowed value '{}', expected one of {:?}",
                        field_name, doc.frontmatter.id, val_str, rule.allowed_values
                    ),
                    node_id: Some(doc.frontmatter.id.clone()),
                    path: Some(doc.relative_path.clone()),
                    remediation: Some(format!(
                        "Use one of the allowed values: {}",
                        rule.allowed_values.join(", ")
                    )),
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    struct TempDir(std::path::PathBuf);
    impl TempDir {
        fn path(&self) -> &std::path::Path {
            &self.0
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn tempdir() -> Result<TempDir, std::io::Error> {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let id = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir =
            std::env::temp_dir().join(format!("akashic-lint-test-{}-{}", std::process::id(), id));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir)?;
        Ok(TempDir(dir))
    }

    #[test]
    fn test_lint_rules() {
        let config = LintConfig::default();
        let vault_config = VaultConfig::default();
        let linter = Linter::new(config, vault_config);

        let diags = linter.check_content("test.md", "hello world");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule, "L001");
    }

    #[test]
    fn test_l001_malformed_frontmatter() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        let bad_content = "This file does not have frontmatter delimiters.\nJust plain text.";
        fs::write(root.join("bad.md"), bad_content).unwrap();

        let graph = VaultGraph::from_dir(root).unwrap();
        let report = lint_vault(&graph);
        assert!(!report.valid);
        assert!(report.error_count >= 1);
        let l001 = report.issues.iter().find(|i| i.code == "L001");
        assert!(l001.is_some());
        assert_eq!(l001.unwrap().severity, Severity::Error);
    }

    #[test]
    fn test_l002_missing_required_fields() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        // Missing id, type, status
        let content = r#"---
title: Incomplete Note
---
# Content
"#;
        fs::write(root.join("incomplete.md"), content).unwrap();

        let graph = VaultGraph::from_dir(root).unwrap();
        let report = lint_vault(&graph);
        assert!(!report.valid);
        let l002_issues: Vec<_> = report.issues.iter().filter(|i| i.code == "L002").collect();
        assert!(!l002_issues.is_empty());
        assert_eq!(l002_issues[0].severity, Severity::Error);
    }

    #[test]
    fn test_l003_duplicate_node_id() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        let note1 = r#"---
id: dup-node-1
title: Note One
type: concept
status: active
---
# Note 1
"#;
        let note2 = r#"---
id: dup-node-1
title: Note Two
type: concept
status: active
---
# Note 2
"#;
        fs::write(root.join("note1.md"), note1).unwrap();
        fs::write(root.join("note2.md"), note2).unwrap();

        let graph = VaultGraph::from_dir(root).unwrap();
        let report = lint_vault(&graph);
        assert!(!report.valid);
        let l003 = report.issues.iter().find(|i| i.code == "L003");
        assert!(l003.is_some());
        assert_eq!(l003.unwrap().severity, Severity::Error);
        assert_eq!(l003.unwrap().node_id.as_deref(), Some("dup-node-1"));
    }

    #[test]
    fn test_l004_broken_relation_target() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        let note = r#"---
id: broken-source
title: Broken Targets
type: decision
status: active
valid_from: "2026-01-01"
relations:
  - type: depends_on
    target: missing-target-id
  - type: relates_to
    target: broken-source
    evidence: nonexistent-evidence.md
---
# Content
"#;
        fs::write(root.join("broken.md"), note).unwrap();

        let graph = VaultGraph::from_dir(root).unwrap();
        let report = lint_vault(&graph);
        assert!(!report.valid);
        let l004_issues: Vec<_> = report.issues.iter().filter(|i| i.code == "L004").collect();
        assert!(l004_issues.len() >= 2);
    }

    #[test]
    fn test_l005_supersedes_and_depends_on_cycles() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        let note_a = r#"---
id: cycle-a
title: Cycle A
type: decision
status: draft
relations:
  - type: supersedes
    target: cycle-b
---
# A
"#;
        let note_b = r#"---
id: cycle-b
title: Cycle B
type: decision
status: draft
relations:
  - type: supersedes
    target: cycle-a
---
# B
"#;
        let note_c = r#"---
id: cycle-c
title: Cycle C
type: concept
status: draft
relations:
  - type: depends_on
    target: cycle-d
---
# C
"#;
        let note_d = r#"---
id: cycle-d
title: Cycle D
type: concept
status: draft
relations:
  - type: depends_on
    target: cycle-c
---
# D
"#;
        fs::write(root.join("a.md"), note_a).unwrap();
        fs::write(root.join("b.md"), note_b).unwrap();
        fs::write(root.join("c.md"), note_c).unwrap();
        fs::write(root.join("d.md"), note_d).unwrap();

        let graph = VaultGraph::from_dir(root).unwrap();
        let report = lint_vault(&graph);
        assert!(!report.valid);
        let l005_issues: Vec<_> = report.issues.iter().filter(|i| i.code == "L005").collect();
        assert_eq!(l005_issues.len(), 2);
    }

    #[test]
    fn test_l006_superseded_without_incoming_edge() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        let orphan = r#"---
id: orphan-superseded
title: Orphan Superseded
type: decision
status: superseded
---
# Orphan
"#;
        fs::write(root.join("orphan.md"), orphan).unwrap();

        let graph = VaultGraph::from_dir(root).unwrap();
        let report = lint_vault(&graph);
        let l006 = report.issues.iter().find(|i| i.code == "L006");
        assert!(l006.is_some());
        assert_eq!(l006.unwrap().severity, Severity::Warning);
    }

    #[test]
    fn test_l007_competing_active_successors_conflict() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        let old = r#"---
id: old-guidance
title: Old Guidance
type: decision
status: superseded
---
# Old
"#;
        let succ1 = r#"---
id: succ-1
title: Successor 1
type: decision
status: active
valid_from: "2026-01-01"
relations:
  - type: supersedes
    target: old-guidance
    evidence: "https://example.com/ev1"
---
# Succ 1
"#;
        let succ2 = r#"---
id: succ-2
title: Successor 2
type: decision
status: active
valid_from: "2026-01-01"
relations:
  - type: supersedes
    target: old-guidance
    evidence: "https://example.com/ev2"
---
# Succ 2
"#;
        fs::write(root.join("old.md"), old).unwrap();
        fs::write(root.join("succ1.md"), succ1).unwrap();
        fs::write(root.join("succ2.md"), succ2).unwrap();

        let graph = VaultGraph::from_dir(root).unwrap();
        let report = lint_vault(&graph);
        assert!(!report.valid);
        let l007 = report.issues.iter().find(|i| i.code == "L007");
        assert!(l007.is_some());
        assert_eq!(l007.unwrap().severity, Severity::Error);
        assert!(l007.unwrap().message.contains("Competing active successors"));
    }

    #[test]
    fn test_l008_unscoped_contradiction() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        let note1 = r#"---
id: claim-1
title: Claim 1
type: concept
status: draft
relations:
  - type: contradicts
    target: claim-2
---
# Claim 1
"#;
        let note2 = r#"---
id: claim-2
title: Claim 2
type: concept
status: draft
---
# Claim 2
"#;
        fs::write(root.join("c1.md"), note1).unwrap();
        fs::write(root.join("c2.md"), note2).unwrap();

        let graph = VaultGraph::from_dir(root).unwrap();
        let report = lint_vault(&graph);
        let l008 = report.issues.iter().find(|i| i.code == "L008");
        assert!(l008.is_some());
        assert_eq!(l008.unwrap().severity, Severity::Warning);
    }

    #[test]
    fn test_l009_missing_valid_from_on_active_guidance() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        let active_nodate = r#"---
id: active-no-date
title: Active Without Valid From
type: guide
status: active
---
# Guide
"#;
        fs::write(root.join("guide.md"), active_nodate).unwrap();

        let graph = VaultGraph::from_dir(root).unwrap();
        let report = lint_vault(&graph);
        let l009 = report.issues.iter().find(|i| i.code == "L009");
        assert!(l009.is_some());
        assert_eq!(l009.unwrap().severity, Severity::Warning);
    }

    #[test]
    fn test_l010_high_impact_relation_missing_evidence() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        let old = r#"---
id: old-target
title: Old Target
type: decision
status: draft
---
# Old
"#;
        let new_note = r#"---
id: new-decision
title: New Decision
type: decision
status: draft
relations:
  - type: supersedes
    target: old-target
---
# New
"#;
        fs::write(root.join("old.md"), old).unwrap();
        fs::write(root.join("new.md"), new_note).unwrap();

        let graph = VaultGraph::from_dir(root).unwrap();
        let report = lint_vault(&graph);
        let l010 = report.issues.iter().find(|i| i.code == "L010");
        assert!(l010.is_some());
        assert_eq!(l010.unwrap().severity, Severity::Warning);
    }

    #[test]
    fn test_l011_prohibited_reverse_relations() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        let note = r#"---
id: proh-test
title: Prohibited Relation Note
type: decision
status: draft
superseded_by: newer-node
relations:
  - type: resolved_by
    target: inc-resolved
---
# Prohibited Note
"#;
        fs::write(root.join("proh.md"), note).unwrap();

        let graph = VaultGraph::from_dir(root).unwrap();
        let report = lint_vault(&graph);
        assert!(!report.valid);
        let l011_issues: Vec<_> = report.issues.iter().filter(|i| i.code == "L011").collect();
        assert_eq!(l011_issues.len(), 2);
        assert_eq!(l011_issues[0].severity, Severity::Error);
    }

    #[test]
    fn test_l012_custom_field_rules_validation() {
        let toml_config = r#"
[lint.custom_fields.owner_team]
required = true
type = "string"

[lint.custom_fields.sla_tier]
type = "integer"
allowed_values = ["1", "2", "3"]

[lint.custom_fields.environment]
type = "enum"
allowed_values = ["production", "staging"]

[lint.custom_fields.decision_scope]
type = "string"
applies_to = ["decision"]
required = true
"#;

        let rules = parse_custom_field_rules_from_toml(toml_config).unwrap();
        assert_eq!(rules.len(), 4);
        assert!(rules.get("owner_team").unwrap().required);
        assert_eq!(rules.get("sla_tier").unwrap().field_type, "integer");

        let dir = tempdir().unwrap();
        let root = dir.path();

        // 1. Missing required owner_team
        let note1 = r#"---
id: note-missing-required
title: Note 1
type: concept
status: draft
---
# N1
"#;
        // 2. Wrong type for sla_tier (string instead of integer)
        let note2 = r#"---
id: note-wrong-type
title: Note 2
type: concept
status: draft
owner_team: platform
sla_tier: "tier-one"
---
# N2
"#;
        // 3. Disallowed enum value for environment
        let note3 = r#"---
id: note-disallowed-enum
title: Note 3
type: concept
status: draft
owner_team: platform
environment: development
---
# N3
"#;
        // 4. Decision note missing decision_scope
        let note4 = r#"---
id: note-decision-missing-scope
title: Note 4
type: decision
status: draft
owner_team: platform
---
# N4
"#;
        // 5. Concept note missing decision_scope (should NOT trigger error because applies_to = ["decision"])
        let note5 = r#"---
id: note-concept-valid
title: Note 5
type: concept
status: draft
owner_team: platform
sla_tier: 2
environment: production
---
# N5
"#;

        fs::write(root.join("n1.md"), note1).unwrap();
        fs::write(root.join("n2.md"), note2).unwrap();
        fs::write(root.join("n3.md"), note3).unwrap();
        fs::write(root.join("n4.md"), note4).unwrap();
        fs::write(root.join("n5.md"), note5).unwrap();

        let graph = VaultGraph::from_dir(root).unwrap();
        let report = lint_vault_with_rules(&graph, &rules);

        let l012_issues: Vec<_> = report.issues.iter().filter(|i| i.code == "L012").collect();
        assert_eq!(l012_issues.len(), 4);

        // Check node-missing-required
        assert!(l012_issues
            .iter()
            .any(|i| i.node_id.as_deref() == Some("note-missing-required")
                && i.message.contains("owner_team")));

        // Check note-wrong-type
        assert!(l012_issues
            .iter()
            .any(|i| i.node_id.as_deref() == Some("note-wrong-type")
                && i.message.contains("sla_tier")));

        // Check note-disallowed-enum
        assert!(l012_issues
            .iter()
            .any(|i| i.node_id.as_deref() == Some("note-disallowed-enum")
                && i.message.contains("environment")));

        // Check note-decision-missing-scope
        assert!(l012_issues
            .iter()
            .any(|i| i.node_id.as_deref() == Some("note-decision-missing-scope")
                && i.message.contains("decision_scope")));

        // Ensure note-concept-valid has no L012 issues
        assert!(!l012_issues
            .iter()
            .any(|i| i.node_id.as_deref() == Some("note-concept-valid")));
    }
}
