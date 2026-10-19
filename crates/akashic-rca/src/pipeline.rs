use crate::stage4::{MockStage4Executor, Stage4Executor, Stage4Output, Stage4Request};
use crate::store::{
    current_timestamp_secs, IncidentClRecord, StateStore, StormGateOutcome, WorkflowRunRecord,
};
use crate::RcaError;
use akashic_sanitize::{LogProfile, SanitizerProfile};
use akashic_vault::crystallizer::{
    build_evidence_node, build_incident_node, build_resolution_node, write_vault_node,
    EvidenceInput, IncidentInput, ResolutionInput,
};
use akashic_vault::frontmatter::{
    NodeStatus, Relation, RelationType, RemediationType, Scope,
};
use akashic_vault::parser::serialize_document;
use akashic_vault::VaultGraph;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineInput {
    pub run_id: Option<String>,
    #[serde(alias = "session_id")]
    pub legacy_session_id: Option<String>,
    pub jira_key: String,
    pub app_id: String,
    pub device: String,
    pub android_version: String,
    pub raw_logs: String,
    pub vault_path: PathBuf,
    pub graph_store: Option<PathBuf>,
    pub prior_resolution_id: Option<String>,
    pub policy_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct PipelineContext {
    pub run_id: String,
    pub jira_key: String,
    pub app_id: String,
    pub device: String,
    pub android_version: String,
    pub raw_logs: String,
    pub vault_path: String,
    pub graph_store: Option<String>,
    pub prior_resolution_id: Option<String>,
    pub policy_path: Option<String>,

    pub crash_signature: Option<String>,
    pub deduplicated: bool,
    pub incident_id: Option<String>,
    pub sanitized_logs: Option<String>,
    pub redactions_count: usize,
    pub symbolication_output: Option<crate::symbolicator::SymbolicationResult>,
    pub stage4_output: Option<Stage4Output>,
    pub incident_node_id: Option<String>,
    pub evidence_node_id: Option<String>,
    pub resolution_node_id: Option<String>,
}

impl PipelineContext {
    pub fn session_id(&self) -> &str {
        &self.run_id
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GerritEvent {
    PatchsetCreated {
        incident_id: String,
        change_id: String,
        cl_number: i64,
        patchset_number: i64,
    },
    ChangeMerged {
        incident_id: String,
        change_id: String,
        cl_number: i64,
    },
    ChangeAbandoned {
        incident_id: String,
        change_id: String,
        cl_number: i64,
        reason: String,
    },
}

pub struct ConductorPipeline<E: Stage4Executor = MockStage4Executor> {
    state_store: StateStore,
    stage4_executor: E,
}

pub type Pipeline<E> = ConductorPipeline<E>;

impl<E: Stage4Executor> ConductorPipeline<E> {
    pub fn new(state_store: StateStore, stage4_executor: E) -> Self {
        Self {
            state_store,
            stage4_executor,
        }
    }

    pub fn state_store(&self) -> &StateStore {
        &self.state_store
    }

    pub fn stage4_executor(&self) -> &E {
        &self.stage4_executor
    }

    pub fn stage4_runner(&self) -> &E {
        &self.stage4_executor
    }

    // Stage 1: Ingest crash input, device context, logs, and metadata
    pub fn run_stage1(&self, context: &mut PipelineContext) -> Result<(), RcaError> {
        let now = current_timestamp_secs();
        let ctx_json = serde_json::to_string(&context)?;

        let run_record = WorkflowRunRecord {
            run_id: context.run_id.clone(),
            jira_key: context.jira_key.clone(),
            crash_signature: context.crash_signature.clone(),
            status: "IN_PROGRESS".to_string(),
            current_stage: "CREATED".to_string(),
            created_at: now,
            updated_at: now,
            context_json: ctx_json.clone(),
        };

        if self.state_store.get_workflow_run(&context.run_id)?.is_none() {
            self.state_store.create_workflow_run(&run_record)?;
        }

        self.state_store
            .save_checkpoint(&context.run_id, "CREATED", &ctx_json)?;

        Ok(())
    }

    // Stage 2: Sanitize logs using LogProfile
    pub fn run_stage2(&self, context: &mut PipelineContext) -> Result<(), RcaError> {
        let sanitizer = LogProfile::new();
        let sanitized = sanitizer.sanitize(&context.raw_logs);

        context.sanitized_logs = Some(sanitized.sanitized_content);
        context.redactions_count = sanitized.redactions_count;

        let ctx_json = serde_json::to_string(&context)?;
        self.state_store.update_workflow_run(
            &context.run_id,
            "SANITIZED",
            "IN_PROGRESS",
            context.crash_signature.as_deref(),
            &ctx_json,
        )?;
        self.state_store
            .save_checkpoint(&context.run_id, "SANITIZED", &ctx_json)?;

        Ok(())
    }

    // Stage 3: Symbolicate & Storm Gate Deduplication
    pub fn run_stage3(&self, context: &mut PipelineContext) -> Result<bool, RcaError> {
        let logs_to_symbolicate = context
            .sanitized_logs
            .as_deref()
            .unwrap_or(&context.raw_logs);

        // Build symbol resolver
        let resolver: Box<dyn crate::symbolicator::SymbolResolver> =
            if let Some(ref db_path) = context.graph_store {
                let p = Path::new(db_path);
                if p.exists() {
                    match akashic_engine::GraphStore::open(p) {
                        Ok(store) => Box::new(crate::symbolicator::EngineSymbolResolver::new(store)),
                        Err(_) => Box::new(crate::symbolicator::InMemorySymbolResolver::with_motorola_defaults()),
                    }
                } else {
                    Box::new(crate::symbolicator::InMemorySymbolResolver::with_motorola_defaults())
                }
            } else {
                Box::new(crate::symbolicator::InMemorySymbolResolver::with_motorola_defaults())
            };

        let sym_result = crate::symbolicator::symbolicate_crash(
            &context.app_id,
            logs_to_symbolicate,
            resolver.as_ref(),
        )?;
        let sig = sym_result.crash_signature.clone();

        context.crash_signature = Some(sig.clone());
        context.symbolication_output = Some(sym_result);

        // Check storm gate
        let incident_id = format!("INC-{}", context.jira_key);
        let outcome = self
            .state_store
            .check_or_register_storm_signature(&sig, &context.run_id, &incident_id)?;

        match outcome {
            StormGateOutcome::Deduplicated {
                incident_id: existing_inc_id,
                hit_count,
            } => {
                context.deduplicated = true;
                context.incident_id = Some(existing_inc_id.clone());

                // Write supplemental EVD node linking to existing incident
                let evd_id = format!("EVD-{}-STACK", context.jira_key);
                let vault_root = Path::new(&context.vault_path);

                let mut scope = Scope::new();
                scope.insert("app".to_string(), context.app_id.clone());
                if !context.device.is_empty() {
                    scope.insert("device".to_string(), context.device.clone());
                }

                let stack_body = context
                    .symbolication_output
                    .as_ref()
                    .map(|o| format!("```\n{}\n```", o.symbolicated_stack_trace))
                    .unwrap_or_else(|| "```\nNo stack trace\n```".to_string());

                let evd_input = EvidenceInput {
                    id: evd_id.clone(),
                    title: format!(
                        "Supplemental stack trace for {} (storm hit #{})",
                        context.jira_key, hit_count
                    ),
                    incident_id: Some(existing_inc_id.clone()),
                    status: Some(NodeStatus::Active),
                    scope,
                    invalidated_hypotheses: vec![],
                    failed_attempts: vec![],
                    relations: vec![],
                    body: stack_body,
                };
                let mut artifact = build_evidence_node(evd_input)?;
                artifact.frontmatter.valid_from = Some("2026-10-19".to_string());
                artifact.rendered_markdown = serialize_document(&artifact.frontmatter, &artifact.body)?;
                write_vault_node(vault_root, "evidence", &artifact)?;

                context.evidence_node_id = Some(evd_id);

                let ctx_json = serde_json::to_string(&context)?;
                self.state_store.update_workflow_run(
                    &context.run_id,
                    "DEDUPLICATED",
                    "DEDUPLICATED",
                    Some(&sig),
                    &ctx_json,
                )?;
                self.state_store
                    .save_checkpoint(&context.run_id, "DEDUPLICATED", &ctx_json)?;

                Ok(true)
            }
            StormGateOutcome::NewRegistered => {
                context.deduplicated = false;
                context.incident_id = Some(incident_id);

                let ctx_json = serde_json::to_string(&context)?;
                self.state_store.update_workflow_run(
                    &context.run_id,
                    "SYMBOLICATED",
                    "IN_PROGRESS",
                    Some(&sig),
                    &ctx_json,
                )?;
                self.state_store
                    .save_checkpoint(&context.run_id, "SYMBOLICATED", &ctx_json)?;

                Ok(false)
            }
        }
    }

    // Stage 4: Investigation & Hypothesis Generation
    pub async fn run_stage4(&self, context: &mut PipelineContext) -> Result<(), RcaError> {
        let sym_evidence = context
            .symbolication_output
            .as_ref()
            .map(|s| serde_json::to_value(s).unwrap_or(serde_json::Value::Null))
            .unwrap_or(serde_json::Value::Null);

        let request = Stage4Request {
            run_id: context.run_id.clone(),
            jira_key: context.jira_key.clone(),
            app_id: context.app_id.clone(),
            crash_signature: context.crash_signature.clone().unwrap_or_default(),
            sanitized_logs: context.sanitized_logs.clone().unwrap_or_default(),
            symbolication_evidence: sym_evidence,
            policy_path: context.policy_path.as_ref().map(PathBuf::from),
        };

        let output = self.stage4_executor.execute(&request).await?;
        context.stage4_output = Some(output);

        let ctx_json = serde_json::to_string(&context)?;
        self.state_store.update_workflow_run(
            &context.run_id,
            "HYPOTHESIS_FALSIFIED",
            "IN_PROGRESS",
            context.crash_signature.as_deref(),
            &ctx_json,
        )?;
        self.state_store
            .save_checkpoint(&context.run_id, "HYPOTHESIS_FALSIFIED", &ctx_json)?;

        Ok(())
    }

    // Stage 5: Crystallize & akashic-lint Gate
    pub fn run_stage5(&self, context: &mut PipelineContext) -> Result<(), RcaError> {
        let vault_root = Path::new(&context.vault_path);

        let incident_id = context
            .incident_id
            .clone()
            .unwrap_or_else(|| format!("INC-{}", context.jira_key));
        let evidence_id = format!("EVD-{}-STACK", context.jira_key);
        let resolution_id = format!("RES-{}-01", context.jira_key);

        let mut scope = Scope::new();
        scope.insert("app".to_string(), context.app_id.clone());
        if !context.device.is_empty() {
            scope.insert("device".to_string(), context.device.clone());
        }
        if !context.android_version.is_empty() {
            scope.insert("android_version".to_string(), context.android_version.clone());
        }
        if let Some(ref sym) = context.symbolication_output {
            if let Some(ref fault) = sym.faulting_symbol {
                scope.insert("repo".to_string(), fault.repo_name.clone());
            }
        }

        let primary_hyp = context
            .stage4_output
            .as_ref()
            .map(|o| o.primary_hypothesis.clone())
            .unwrap_or_else(|| "Incident investigation".to_string());

        let invalidated_hyps: Vec<String> = context
            .stage4_output
            .as_ref()
            .map(|o| {
                o.invalidated_hypotheses
                    .iter()
                    .map(|h| format!("{}: {}", h.hypothesis_id, h.description))
                    .collect()
            })
            .unwrap_or_default();

        let failed_attempts: Vec<String> = context
            .stage4_output
            .as_ref()
            .map(|o| o.failed_attempts.clone())
            .unwrap_or_default();

        let falsifications = context
            .stage4_output
            .as_ref()
            .map(|o| o.falsification_attempts.join("\n"))
            .unwrap_or_default();

        let remediation = context
            .stage4_output
            .as_ref()
            .map(|o| o.remediation_proposal.clone())
            .unwrap_or_default();

        // 1. INCIDENT node
        let incident_body = format!(
            "### Hypothesis\n{}\n\n### Falsification Attempts\n{}\n\n### Remediation Proposal\n{}\n",
            primary_hyp, falsifications, remediation
        );
        let mut incident_artifact = build_incident_node(IncidentInput {
            id: incident_id.clone(),
            title: format!("Crash in {}: {}", context.app_id, primary_hyp),
            crash_signature: context.crash_signature.clone().unwrap_or_default(),
            status: Some(NodeStatus::Active),
            scope: scope.clone(),
            external_refs: None,
            relations: vec![Relation {
                target: evidence_id.clone(),
                relation_type: RelationType::SupportedBy,
                evidence: None,
                scope: scope.clone(),
            }],
            body: incident_body,
        })?;
        incident_artifact.frontmatter.valid_from = Some("2026-10-19".to_string());
        incident_artifact.frontmatter.invalidated_hypotheses = invalidated_hyps.clone();
        incident_artifact.frontmatter.failed_attempts = failed_attempts.clone();
        incident_artifact.rendered_markdown =
            serialize_document(&incident_artifact.frontmatter, &incident_artifact.body)?;
        write_vault_node(vault_root, "incidents", &incident_artifact)?;

        // 2. EVIDENCE node
        let stack_trace_body = context
            .symbolication_output
            .as_ref()
            .map(|o| format!("```\n{}\n```", o.symbolicated_stack_trace))
            .unwrap_or_else(|| "```\nNo stack trace\n```".to_string());
        let mut evidence_artifact = build_evidence_node(EvidenceInput {
            id: evidence_id.clone(),
            title: format!("Symbolicated stack trace for {}", context.jira_key),
            incident_id: Some(incident_id.clone()),
            status: Some(NodeStatus::Active),
            scope: scope.clone(),
            invalidated_hypotheses: invalidated_hyps.clone(),
            failed_attempts: failed_attempts.clone(),
            relations: vec![Relation {
                target: incident_id.clone(),
                relation_type: RelationType::Substantiates,
                evidence: None,
                scope: scope.clone(),
            }],
            body: stack_trace_body,
        })?;
        evidence_artifact.frontmatter.valid_from = Some("2026-10-19".to_string());
        evidence_artifact.rendered_markdown =
            serialize_document(&evidence_artifact.frontmatter, &evidence_artifact.body)?;
        write_vault_node(vault_root, "evidence", &evidence_artifact)?;

        // 3. RESOLUTION node
        let mut res_relations = vec![Relation {
            target: incident_id.clone(),
            relation_type: RelationType::Resolves,
            evidence: Some(evidence_id.clone()),
            scope: scope.clone(),
        }];

        if let Some(ref prior_id) = context.prior_resolution_id {
            res_relations.push(Relation {
                target: prior_id.clone(),
                relation_type: RelationType::Supersedes,
                evidence: Some(evidence_id.clone()),
                scope: scope.clone(),
            });
        }

        let resolution_body = format!("### Remediation Plan\n{}\n", remediation);
        let mut resolution_artifact = build_resolution_node(ResolutionInput {
            id: resolution_id.clone(),
            title: format!("Resolution for {}", context.jira_key),
            incident_id: Some(incident_id.clone()),
            remediation_type: RemediationType::PermanentFix,
            status: Some(NodeStatus::Active),
            abandoned_reason: None,
            scope: scope.clone(),
            external_refs: None,
            relations: res_relations,
            body: resolution_body,
        })?;
        resolution_artifact.frontmatter.valid_from = Some("2026-10-19".to_string());
        resolution_artifact.rendered_markdown =
            serialize_document(&resolution_artifact.frontmatter, &resolution_artifact.body)?;
        write_vault_node(vault_root, "resolutions", &resolution_artifact)?;

        // Lint gate: validate vault graph
        let graph = VaultGraph::from_dir(vault_root)?;
        let report = akashic_lint::lint_vault(&graph);
        if !report.valid {
            return Err(RcaError::VaultLintFailed(
                report.error_count,
                report.issues,
            ));
        }

        context.incident_node_id = Some(incident_id.clone());
        context.evidence_node_id = Some(evidence_id);
        context.resolution_node_id = Some(resolution_id.clone());

        if let Ok(Some(mut cl_rec)) = self.state_store.get_incident_cl(&incident_id) {
            cl_rec.resolution_id = resolution_id;
            if cl_rec.repo_name.is_empty() {
                if let Some(repo) = scope.get("repo") {
                    cl_rec.repo_name = repo.clone();
                }
            }
            cl_rec.updated_at = current_timestamp_secs();
            let _ = self.state_store.upsert_incident_cl(&cl_rec);
        }

        let ctx_json = serde_json::to_string(&context)?;
        self.state_store.update_workflow_run(
            &context.run_id,
            "CRYSTALLIZED",
            "IN_PROGRESS",
            context.crash_signature.as_deref(),
            &ctx_json,
        )?;
        self.state_store
            .save_checkpoint(&context.run_id, "CRYSTALLIZED", &ctx_json)?;

        Ok(())
    }

    // Stage 6: Resolution tracking and finalization
    pub fn run_stage6(&self, context: &mut PipelineContext) -> Result<(), RcaError> {
        let ctx_json = serde_json::to_string(&context)?;
        self.state_store.update_workflow_run(
            &context.run_id,
            "RESOLVED",
            "COMPLETED",
            context.crash_signature.as_deref(),
            &ctx_json,
        )?;
        self.state_store
            .save_checkpoint(&context.run_id, "RESOLVED", &ctx_json)?;

        Ok(())
    }

    fn resolve_incident_meta(&self, incident_id: &str, vault_path: &Path) -> (String, String) {
        // 1. Check existing incident_cl record
        if let Ok(Some(record)) = self.state_store.get_incident_cl(incident_id) {
            if !record.resolution_id.is_empty() {
                return (record.resolution_id, record.repo_name);
            }
        }

        let mut res_id = String::new();
        let mut repo = String::new();

        // 2. Scan vault resolutions
        let res_dir = vault_path.join("resolutions");
        if res_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&res_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) == Some("md") {
                        if let Ok(doc) = akashic_vault::parse_markdown_file(vault_path, &path) {
                            let resolves = doc.frontmatter.relations.iter().any(|r| {
                                r.relation_type == RelationType::Resolves
                                    && (r.target == incident_id
                                        || r.target.starts_with(incident_id)
                                        || incident_id.starts_with(&r.target)
                                        || r.target == format!("../incidents/{}.md", incident_id))
                            });
                            if resolves {
                                res_id = doc.frontmatter.id.clone();
                                if let Some(r) = doc.frontmatter.scope.get("repo") {
                                    repo = r.clone();
                                }
                                break;
                            }
                        }
                    }
                }
            }
        }

        // 3. Fallback resolution id if none found
        if res_id.is_empty() {
            if let Some(stripped) = incident_id.strip_prefix("INC-") {
                if stripped.ends_with("-01") {
                    res_id = format!("RES-{}", stripped);
                } else {
                    res_id = format!("RES-{}-01", stripped);
                }
            } else {
                res_id = format!("RES-{}-01", incident_id);
            }
        }

        // 4. Fallback repo from incident node if still empty
        if repo.is_empty() {
            let inc_dir = vault_path.join("incidents");
            if inc_dir.exists() {
                if let Ok(entries) = std::fs::read_dir(&inc_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.extension().and_then(|e| e.to_str()) == Some("md") {
                            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
                            if stem == incident_id || stem.starts_with(incident_id) || incident_id.starts_with(stem) {
                                if let Ok(doc) = akashic_vault::parse_markdown_file(vault_path, &path) {
                                    if let Some(r) = doc.frontmatter.scope.get("repo") {
                                        repo = r.clone();
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        (res_id, repo)
    }

    // Gerrit event handler for CL tracking and negative knowledge
    pub fn handle_gerrit_event(&self, event: &GerritEvent, vault_path: &Path) -> Result<(), RcaError> {
        match event {
            GerritEvent::PatchsetCreated {
                incident_id,
                change_id,
                cl_number,
                patchset_number,
            } => {
                let (resolution_id, repo_name) = self.resolve_incident_meta(incident_id, vault_path);
                self.state_store.upsert_incident_cl(&IncidentClRecord {
                    incident_id: incident_id.clone(),
                    gerrit_change_id: change_id.clone(),
                    gerrit_cl_number: *cl_number,
                    patchset_number: *patchset_number,
                    status: "IN_REVIEW".to_string(),
                    resolution_id,
                    repo_name,
                    updated_at: current_timestamp_secs(),
                })?;
            }
            GerritEvent::ChangeMerged {
                incident_id,
                change_id,
                cl_number,
            } => {
                let (resolution_id, repo_name) = self.resolve_incident_meta(incident_id, vault_path);
                self.state_store.upsert_incident_cl(&IncidentClRecord {
                    incident_id: incident_id.clone(),
                    gerrit_change_id: change_id.clone(),
                    gerrit_cl_number: *cl_number,
                    patchset_number: 1,
                    status: "MERGED".to_string(),
                    resolution_id: resolution_id.clone(),
                    repo_name: repo_name.clone(),
                    updated_at: current_timestamp_secs(),
                })?;

                // Update resolution node status to Resolved
                let res_dir = vault_path.join("resolutions");
                if res_dir.exists() {
                    if let Ok(entries) = std::fs::read_dir(&res_dir) {
                        for entry in entries.flatten() {
                            let path = entry.path();
                            if path.extension().and_then(|e| e.to_str()) == Some("md") {
                                if let Ok(doc) = akashic_vault::parse_markdown_file(vault_path, &path) {
                                    let resolves_target = doc.frontmatter.id == resolution_id
                                        || doc.frontmatter.relations.iter().any(|r| {
                                            r.relation_type == RelationType::Resolves
                                                && (r.target == *incident_id
                                                    || r.target.starts_with(incident_id)
                                                    || r.target == format!("../incidents/{}.md", incident_id))
                                        });
                                    if resolves_target {
                                        let mut artifact = build_resolution_node(ResolutionInput {
                                            id: doc.frontmatter.id.clone(),
                                            title: doc.frontmatter.title.clone(),
                                            incident_id: Some(incident_id.clone()),
                                            remediation_type: doc.frontmatter.remediation_type.unwrap_or(RemediationType::PermanentFix),
                                            status: Some(NodeStatus::Resolved),
                                            abandoned_reason: doc.frontmatter.abandoned_reason.clone(),
                                            scope: doc.frontmatter.scope.clone(),
                                            external_refs: doc.frontmatter.external_refs.clone(),
                                            relations: doc.frontmatter.relations.clone(),
                                            body: doc.body.clone(),
                                        })?;
                                        artifact.frontmatter.valid_from = doc.frontmatter.valid_from.clone().or_else(|| Some("2026-10-19".to_string()));
                                        artifact.frontmatter.failed_attempts = doc.frontmatter.failed_attempts.clone();
                                        artifact.rendered_markdown = serialize_document(&artifact.frontmatter, &artifact.body)?;
                                        write_vault_node(vault_path, "resolutions", &artifact)?;
                                    }
                                }
                            }
                        }
                    }
                }

                // Lint gate: validate vault graph
                let graph = VaultGraph::from_dir(vault_path)?;
                let report = akashic_lint::lint_vault(&graph);
                if !report.valid {
                    return Err(RcaError::VaultLintFailed(
                        report.error_count,
                        report.issues,
                    ));
                }
            }
            GerritEvent::ChangeAbandoned {
                incident_id,
                change_id,
                cl_number,
                reason,
            } => {
                let (resolution_id, repo_name) = self.resolve_incident_meta(incident_id, vault_path);
                self.state_store.upsert_incident_cl(&IncidentClRecord {
                    incident_id: incident_id.clone(),
                    gerrit_change_id: change_id.clone(),
                    gerrit_cl_number: *cl_number,
                    patchset_number: 1,
                    status: "ABANDONED".to_string(),
                    resolution_id: resolution_id.clone(),
                    repo_name: repo_name.clone(),
                    updated_at: current_timestamp_secs(),
                })?;

                // Record negative knowledge into the vault resolution node
                let res_dir = vault_path.join("resolutions");
                if res_dir.exists() {
                    if let Ok(entries) = std::fs::read_dir(&res_dir) {
                        for entry in entries.flatten() {
                            let path = entry.path();
                            if path.extension().and_then(|e| e.to_str()) == Some("md") {
                                if let Ok(doc) = akashic_vault::parse_markdown_file(vault_path, &path) {
                                    let resolves_target = doc.frontmatter.id == resolution_id
                                        || doc.frontmatter.relations.iter().any(|r| {
                                            r.relation_type == RelationType::Resolves
                                                && (r.target == *incident_id
                                                    || r.target.starts_with(incident_id)
                                                    || r.target == format!("../incidents/{}.md", incident_id))
                                        });
                                    if resolves_target {
                                        let mut failed_attempts = doc.frontmatter.failed_attempts.clone();
                                        failed_attempts.push(format!("CL {} abandoned: {}", cl_number, reason));

                                        let mut artifact = build_resolution_node(ResolutionInput {
                                            id: doc.frontmatter.id.clone(),
                                            title: doc.frontmatter.title.clone(),
                                            incident_id: Some(incident_id.clone()),
                                            remediation_type: doc.frontmatter.remediation_type.unwrap_or(RemediationType::PermanentFix),
                                            status: Some(NodeStatus::Abandoned),
                                            abandoned_reason: Some(reason.clone()),
                                            scope: doc.frontmatter.scope.clone(),
                                            external_refs: doc.frontmatter.external_refs.clone(),
                                            relations: doc.frontmatter.relations.clone(),
                                            body: doc.body.clone(),
                                        })?;
                                        artifact.frontmatter.valid_from = doc.frontmatter.valid_from.clone().or_else(|| Some("2026-10-19".to_string()));
                                        artifact.frontmatter.failed_attempts = failed_attempts;
                                        artifact.rendered_markdown = serialize_document(&artifact.frontmatter, &artifact.body)?;
                                        write_vault_node(vault_path, "resolutions", &artifact)?;
                                    }
                                }
                            }
                        }
                    }
                }

                // Lint gate: validate vault graph
                let graph = VaultGraph::from_dir(vault_path)?;
                let report = akashic_lint::lint_vault(&graph);
                if !report.valid {
                    return Err(RcaError::VaultLintFailed(
                        report.error_count,
                        report.issues,
                    ));
                }
            }
        }
        Ok(())
    }

    // Complete pipeline execution
    pub async fn run(&self, input: PipelineInput) -> Result<PipelineContext, RcaError> {
        let run_id = input
            .run_id
            .or(input.legacy_session_id)
            .unwrap_or_else(|| {
                format!(
                    "run-{}",
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_nanos()
                )
            });

        let mut context = PipelineContext {
            run_id: run_id.clone(),
            jira_key: input.jira_key.clone(),
            app_id: input.app_id.clone(),
            device: input.device.clone(),
            android_version: input.android_version.clone(),
            raw_logs: input.raw_logs.clone(),
            vault_path: input.vault_path.to_string_lossy().to_string(),
            graph_store: input.graph_store.map(|p| p.to_string_lossy().to_string()),
            prior_resolution_id: input.prior_resolution_id,
            policy_path: input.policy_path.map(|p| p.to_string_lossy().to_string()),
            ..PipelineContext::default()
        };

        // Stage 1: Ingest
        self.run_stage1(&mut context)?;

        // Stage 2: Sanitize
        self.run_stage2(&mut context)?;

        // Stage 3: Symbolicate & Storm Gate Dedup
        let is_dedup = self.run_stage3(&mut context)?;
        if is_dedup {
            return Ok(context);
        }

        // Stage 4: Hypothesis Generation & Falsification
        self.run_stage4(&mut context).await?;

        // Stage 5: Crystallize & Lint Gate
        self.run_stage5(&mut context)?;

        // Stage 6: Resolution & Finalize
        self.run_stage6(&mut context)?;

        Ok(context)
    }

    // Resumption support
    pub async fn resume_run(
        &self,
        run_id: &str,
        vault_path_override: Option<&Path>,
        graph_store_override: Option<&Path>,
    ) -> Result<PipelineContext, RcaError> {
        let latest_checkpoint = self
            .state_store
            .get_latest_checkpoint(run_id)?
            .ok_or_else(|| {
                RcaError::Session(format!("No checkpoints found for run {}", run_id))
            })?;

        let mut context: PipelineContext =
            serde_json::from_str(&latest_checkpoint.checkpoint_data_json)?;

        if let Some(vp) = vault_path_override {
            context.vault_path = vp.to_string_lossy().to_string();
        }
        if let Some(gs) = graph_store_override {
            context.graph_store = Some(gs.to_string_lossy().to_string());
        }

        match latest_checkpoint.stage_name.as_str() {
            "RESOLVED" | "DEDUPLICATED" => Ok(context),
            "CRYSTALLIZED" => {
                self.run_stage6(&mut context)?;
                Ok(context)
            }
            "HYPOTHESIS_FALSIFIED" => {
                self.run_stage5(&mut context)?;
                self.run_stage6(&mut context)?;
                Ok(context)
            }
            "SYMBOLICATED" => {
                self.run_stage4(&mut context).await?;
                self.run_stage5(&mut context)?;
                self.run_stage6(&mut context)?;
                Ok(context)
            }
            "SANITIZED" => {
                let is_dedup = self.run_stage3(&mut context)?;
                if is_dedup {
                    return Ok(context);
                }
                self.run_stage4(&mut context).await?;
                self.run_stage5(&mut context)?;
                self.run_stage6(&mut context)?;
                Ok(context)
            }
            "CREATED" => {
                self.run_stage2(&mut context)?;
                let is_dedup = self.run_stage3(&mut context)?;
                if is_dedup {
                    return Ok(context);
                }
                self.run_stage4(&mut context).await?;
                self.run_stage5(&mut context)?;
                self.run_stage6(&mut context)?;
                Ok(context)
            }
            other => Err(RcaError::StageFailed(format!(
                "Unknown checkpoint stage for resumption: {}",
                other
            ))),
        }
    }

    pub async fn resume_session(
        &self,
        session_id: &str,
        vault_path_override: Option<&Path>,
        graph_store_override: Option<&Path>,
    ) -> Result<PipelineContext, RcaError> {
        self.resume_run(session_id, vault_path_override, graph_store_override).await
    }
}

impl ConductorPipeline<MockStage4Executor> {
    pub fn with_mock(state_store: StateStore) -> Self {
        Self::new(state_store, MockStage4Executor::new())
    }
}
