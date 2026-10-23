use crate::curation::{
    BlastRadiusContext, CurationGate, CurationReport, CurationVerdict, InvestigationContextPacket,
    PriorResolutionSummary,
};
use crate::stage4::{
    CandidateFinding, FindingOutcome, MockStage4Executor, Stage4Executor, Stage4Output,
    Stage4Request,
};
use crate::store::{
    current_timestamp_secs, CandidateFindingRecord, IncidentClRecord, StateStore,
    StormGateOutcome, WorkflowRunRecord,
};
use crate::RcaError;
use akashic_sanitize::{LogProfile, SanitizerProfile};
use akashic_vault::crystallizer::{
    build_evidence_node, build_resolution_node, write_vault_node,
    EvidenceInput, ResolutionInput,
};
use akashic_vault::frontmatter::{
    NodeStatus, RelationType, RemediationType, Scope,
};
use akashic_vault::migrator::{
    apply_plan_with_validator, generate_unified_diff, sha256_hex, ChangeAction, MigrationPlan,
    PlannedFileChange,
};
use akashic_vault::parser::serialize_document;
use akashic_vault::VaultGraph;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkflowStage {
    Created,
    Sanitized,
    Symbolicated,
    AwaitingReasoning,
    HypothesisFalsified,
    AwaitingCuration,
    Crystallized,
    Inconclusive,
    Resolved,
}

impl WorkflowStage {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Created => "Created",
            Self::Sanitized => "Sanitized",
            Self::Symbolicated => "Symbolicated",
            Self::AwaitingReasoning => "AwaitingReasoning",
            Self::HypothesisFalsified => "HypothesisFalsified",
            Self::AwaitingCuration => "AwaitingCuration",
            Self::Crystallized => "Crystallized",
            Self::Inconclusive => "Inconclusive",
            Self::Resolved => "Resolved",
        }
    }
}

impl std::fmt::Display for WorkflowStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkflowStatus {
    Running,
    AwaitingReasoning,
    AwaitingCuration,
    Inconclusive,
    Completed,
    Deduplicated,
    Failed,
}

impl WorkflowStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Running => "Running",
            Self::AwaitingReasoning => "AwaitingReasoning",
            Self::AwaitingCuration => "AwaitingCuration",
            Self::Inconclusive => "Inconclusive",
            Self::Completed => "Completed",
            Self::Deduplicated => "Deduplicated",
            Self::Failed => "Failed",
        }
    }
}

impl std::fmt::Display for WorkflowStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
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
            context_packet_json: None,
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
        let vault_root = PathBuf::from(&context.vault_path);

        let packet = match self.state_store.get_context_packet(&context.run_id)? {
            Some(pkt_json) => serde_json::from_str(&pkt_json)?,
            None => self.assemble_context_packet(context, &vault_root)?,
        };
        let cand_id = format!("CAND-{}", &context.run_id);
        let affected = context
            .symbolication_output
            .as_ref()
            .and_then(|s| s.faulting_symbol.as_ref())
            .map(|f| vec![f.symbol.clone()])
            .unwrap_or_default();
        let mut finding = context
            .stage4_output
            .as_ref()
            .map(|o| {
                o.clone().into_candidate_finding(
                    cand_id.clone(),
                    context.run_id.clone(),
                    FindingOutcome::Resolved,
                    affected.clone(),
                    Some("permanent_fix".to_string()),
                    context.prior_resolution_id.clone(),
                )
            })
            .unwrap_or_else(|| CandidateFinding {
                candidate_id: Some(cand_id),
                agent_id: None,
                run_id: Some(context.run_id.clone()),
                storm_candidate_id: None,
                outcome: FindingOutcome::Resolved,
                primary_hypothesis: "Incident investigation".to_string(),
                falsification_attempts: vec!["Empirical analysis".to_string()],
                invalidated_hypotheses: vec![],
                failed_attempts: vec![],
                confidence_score: 1.0,
                remediation_proposal: "Permanent fix applied".to_string(),
                remediation_type: Some("permanent_fix".to_string()),
                supersedes_resolution_id: context.prior_resolution_id.clone(),
                hold_for_curation: false,
                affected_symbols: affected,
            });

        if context.prior_resolution_id.is_some() && finding.supersedes_resolution_id.is_none() {
            finding.supersedes_resolution_id = context.prior_resolution_id.clone();
        }

        let report = CurationGate::validate(&packet, &finding, &vault_root)?;
        if report.verdict == CurationVerdict::Rejected {
            if !report.lint_issues.is_empty() {
                let err_count = report
                    .lint_issues
                    .iter()
                    .filter(|i| i.severity == akashic_lint::Severity::Error)
                    .count();
                return Err(RcaError::VaultLintFailed(err_count, report.lint_issues));
            }
            let err_msg = report
                .issues
                .iter()
                .map(|i| i.message.clone())
                .collect::<Vec<_>>()
                .join("; ");
            return Err(RcaError::Curation(format!(
                "Curation gate rejected finding: {}",
                err_msg
            )));
        }

        self.apply_crystallization(context, &report, finding.outcome, &vault_root)?;

        Ok(())
    }

    fn apply_crystallization(
        &self,
        context: &mut PipelineContext,
        report: &CurationReport,
        outcome: FindingOutcome,
        vault_path: &Path,
    ) -> Result<(), RcaError> {
        if let Some(ref plan) = report.migration_plan {
            apply_plan_with_validator(vault_path, plan, |_| Ok(()))?;
        }

        context.incident_node_id = report.incident_node_id.clone();
        context.evidence_node_id = report.evidence_node_id.clone();
        context.resolution_node_id = report.resolution_node_id.clone();

        if let Some(ref incident_id) = context.incident_node_id {
            if let Some(ref resolution_id) = context.resolution_node_id {
                if let Ok(Some(mut cl_rec)) = self.state_store.get_incident_cl(incident_id) {
                    cl_rec.resolution_id = resolution_id.clone();
                    if cl_rec.repo_name.is_empty() {
                        if let Some(ref sym) = context.symbolication_output {
                            if let Some(ref fault) = sym.faulting_symbol {
                                cl_rec.repo_name = fault.repo_name.clone();
                            }
                        }
                    }
                    cl_rec.updated_at = current_timestamp_secs();
                    let _ = self.state_store.upsert_incident_cl(&cl_rec);
                }
            }
        }

        match outcome {
            FindingOutcome::Inconclusive => {
                let ctx_json = serde_json::to_string(&context)?;
                self.state_store.update_workflow_run(
                    &context.run_id,
                    "INCONCLUSIVE",
                    "INCONCLUSIVE",
                    context.crash_signature.as_deref(),
                    &ctx_json,
                )?;
                self.state_store
                    .save_checkpoint(&context.run_id, "INCONCLUSIVE", &ctx_json)?;
            }
            FindingOutcome::Resolved => {
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
                self.run_stage6(context)?;
            }
        }

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

    // Assemble Stage 3.5 InvestigationContextPacket
    pub fn assemble_context_packet(
        &self,
        context: &PipelineContext,
        vault_path: &Path,
    ) -> Result<InvestigationContextPacket, RcaError> {
        let incident_id = context
            .incident_id
            .clone()
            .unwrap_or_else(|| format!("INC-{}", context.jira_key));

        let mut historical_invalidated_hypotheses = Vec::new();
        let mut historical_failed_attempts = Vec::new();
        let mut prior_resolutions = Vec::new();

        let mut seen_hypotheses = HashSet::new();
        let mut seen_attempts = HashSet::new();
        let mut seen_resolutions = HashSet::new();

        if vault_path.exists() {
            let scan_dirs = ["incidents", "evidence", "resolutions"];
            for dir_name in &scan_dirs {
                let dir_path = vault_path.join(dir_name);
                if dir_path.exists() {
                    if let Ok(entries) = std::fs::read_dir(&dir_path) {
                        for entry in entries.flatten() {
                            let path = entry.path();
                            if path.extension().and_then(|e| e.to_str()) == Some("md") {
                                if let Ok(doc) = akashic_vault::parse_markdown_file(vault_path, &path) {
                                    let is_match = doc.frontmatter.id == incident_id
                                        || context.prior_resolution_id.as_deref() == Some(&doc.frontmatter.id)
                                        || doc.frontmatter.scope.get("app") == Some(&context.app_id)
                                        || doc.frontmatter.relations.iter().any(|r| {
                                            r.target == incident_id
                                                || context.prior_resolution_id.as_deref() == Some(&r.target)
                                        });

                                    if is_match {
                                        for h in doc.frontmatter.invalidated_hypotheses {
                                            if seen_hypotheses.insert(h.clone()) {
                                                historical_invalidated_hypotheses.push(h);
                                            }
                                        }
                                        for f in doc.frontmatter.failed_attempts {
                                            if seen_attempts.insert(f.clone()) {
                                                historical_failed_attempts.push(f);
                                            }
                                        }
                                        if *dir_name == "resolutions" || doc.frontmatter.id.starts_with("RES-") {
                                            if seen_resolutions.insert(doc.frontmatter.id.clone()) {
                                                let res_summary = PriorResolutionSummary {
                                                    resolution_id: doc.frontmatter.id.clone(),
                                                    status: format!("{:?}", doc.frontmatter.status).to_lowercase(),
                                                    remediation_type: doc
                                                        .frontmatter
                                                        .remediation_type
                                                        .map(|r| format!("{:?}", r).to_lowercase()),
                                                    abandoned_reason: doc.frontmatter.abandoned_reason.clone(),
                                                };
                                                prior_resolutions.push(res_summary);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        let mut blast_radius = None;
        if let Some(ref gs_path_str) = context.graph_store {
            if let Some(ref sym) = context
                .symbolication_output
                .as_ref()
                .and_then(|s| s.faulting_symbol.as_ref())
            {
                if let Ok(store) = akashic_engine::store::GraphStore::open(Path::new(gs_path_str)) {
                    if let Ok(report) = akashic_engine::analytics::blast_radius(
                        &store,
                        &sym.repo_name,
                        &[sym.symbol.clone()],
                        2,
                    ) {
                        blast_radius = Some(BlastRadiusContext {
                            repo_name: sym.repo_name.clone(),
                            seed_symbol_ids: vec![sym.symbol.clone()],
                            upstream_callers: report
                                .upstream_callers
                                .into_iter()
                                .map(|s| s.id)
                                .collect(),
                            downstream_dependencies: report
                                .downstream_dependencies
                                .into_iter()
                                .map(|s| s.id)
                                .collect(),
                            impacted_communities: report
                                .impacted_communities
                                .into_iter()
                                .map(|c| c.community_id)
                                .collect(),
                        });
                    }
                }
            }
        }

        let symbolication = context
            .symbolication_output
            .clone()
            .unwrap_or_default();

        let sanitized_logs = context
            .sanitized_logs
            .clone()
            .unwrap_or_else(|| context.raw_logs.clone());

        let redactions_count = context.redactions_count;

        let packet = InvestigationContextPacket {
            run_id: context.run_id.clone(),
            incident_id,
            jira_key: context.jira_key.clone(),
            app_id: context.app_id.clone(),
            device: context.device.clone(),
            android_version: context.android_version.clone(),
            crash_signature: context.crash_signature.clone().unwrap_or_default(),
            sanitized_logs,
            redactions_count,
            symbolication,
            blast_radius,
            historical_invalidated_hypotheses,
            historical_failed_attempts,
            prior_resolutions,
            policy_path: context.policy_path.as_ref().map(PathBuf::from),
        };

        Ok(packet)
    }

    // Phase A: Stages 1-3.5 prepare investigation
    pub fn prepare_investigation(
        &self,
        input: PipelineInput,
    ) -> Result<(PipelineContext, Option<InvestigationContextPacket>), RcaError> {
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
            return Ok((context, None));
        }

        // Stage 3.5: Assemble context packet & checkpoint AWAITING_REASONING
        let vault_path = Path::new(&context.vault_path);
        let packet = self.assemble_context_packet(&context, vault_path)?;
        let packet_json = serde_json::to_string(&packet)?;

        self.state_store.save_context_packet(&context.run_id, &packet_json)?;

        let ctx_json = serde_json::to_string(&context)?;
        self.state_store.update_workflow_run(
            &context.run_id,
            "AWAITING_REASONING",
            "AWAITING_REASONING",
            context.crash_signature.as_deref(),
            &ctx_json,
        )?;
        self.state_store
            .save_checkpoint(&context.run_id, "AWAITING_REASONING", &ctx_json)?;

        Ok((context, Some(packet)))
    }

    // Phase B: Stage 4.5 submit finding through CurationGate
    pub fn submit_finding(
        &self,
        run_id: &str,
        finding: &CandidateFinding,
        vault_path: &Path,
    ) -> Result<(PipelineContext, CurationReport), RcaError> {
        let latest_checkpoint = self
            .state_store
            .get_latest_checkpoint(run_id)?
            .ok_or_else(|| {
                RcaError::Session(format!("No checkpoints found for run {}", run_id))
            })?;

        let mut context: PipelineContext =
            serde_json::from_str(&latest_checkpoint.checkpoint_data_json)?;
        context.vault_path = vault_path.to_string_lossy().to_string();

        let packet: InvestigationContextPacket = match self.state_store.get_context_packet(run_id)? {
            Some(pkt_json) => serde_json::from_str(&pkt_json)?,
            None => self.assemble_context_packet(&context, vault_path)?,
        };

        let mut report = CurationGate::validate(&packet, finding, vault_path)?;

        let existing_candidates = self
            .state_store
            .list_candidate_findings(run_id)
            .unwrap_or_default();

        let has_competing = existing_candidates
            .iter()
            .any(|c| c.status != "REJECTED");

        if report.verdict != CurationVerdict::Rejected {
            if finding.hold_for_curation || has_competing {
                report.verdict = CurationVerdict::HeldForCuration;
            }
        }

        let cand_id = finding.candidate_id.clone().unwrap_or_else(|| {
            format!(
                "cand-{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            )
        });
        let finding_json = serde_json::to_string(finding)?;
        let aff_json = serde_json::to_string(&finding.affected_symbols)?;
        let now = current_timestamp_secs();
        let outcome_str = match finding.outcome {
            FindingOutcome::Resolved => "RESOLVED",
            FindingOutcome::Inconclusive => "INCONCLUSIVE",
        };
        let status_str = match report.verdict {
            CurationVerdict::Approved => "ACCEPTED",
            CurationVerdict::HeldForCuration => "PENDING",
            CurationVerdict::Rejected => "REJECTED",
        };

        let record = CandidateFindingRecord {
            candidate_id: cand_id,
            run_id: run_id.to_string(),
            storm_candidate_id: None,
            outcome: outcome_str.to_string(),
            summary: finding.primary_hypothesis.clone(),
            remediation_proposal: finding.remediation_proposal.clone(),
            confidence_score: finding.confidence_score,
            affected_symbols_json: aff_json,
            finding_json,
            status: status_str.to_string(),
            created_at: now,
            updated_at: now,
        };
        self.state_store.stage_candidate_finding(&record)?;

        match report.verdict {
            CurationVerdict::Rejected => {
                // Kept in AWAITING_REASONING, no disk mutations
                Ok((context, report))
            }
            CurationVerdict::HeldForCuration => {
                let ctx_json = serde_json::to_string(&context)?;
                self.state_store.update_workflow_run(
                    &context.run_id,
                    "AWAITING_CURATION",
                    "AWAITING_CURATION",
                    context.crash_signature.as_deref(),
                    &ctx_json,
                )?;
                self.state_store
                    .save_checkpoint(&context.run_id, "AWAITING_CURATION", &ctx_json)?;
                Ok((context, report))
            }
            CurationVerdict::Approved => {
                self.apply_crystallization(&mut context, &report, finding.outcome, vault_path)?;
                Ok((context, report))
            }
        }
    }

    // Adjudicate multi-agent findings held at AWAITING_CURATION
    pub fn curate_investigation(
        &self,
        run_id: &str,
        winning_candidate_id: &str,
        vault_path: &Path,
    ) -> Result<(PipelineContext, CurationReport), RcaError> {
        let candidates = self.state_store.list_candidate_findings(run_id)?;
        let winning_record = candidates
            .iter()
            .find(|c| c.candidate_id == winning_candidate_id)
            .ok_or_else(|| {
                RcaError::Session(format!(
                    "Winning candidate {} not found for run {}",
                    winning_candidate_id, run_id
                ))
            })?;

        let mut winning_finding: CandidateFinding =
            serde_json::from_str(&winning_record.finding_json)?;
        winning_finding.hold_for_curation = false;

        // Fold other candidates' primary_hypothesis into winning finding's invalidated_hypotheses
        for other in &candidates {
            if other.candidate_id != winning_candidate_id {
                if let Ok(other_f) = serde_json::from_str::<CandidateFinding>(&other.finding_json) {
                    if !other_f.primary_hypothesis.is_empty()
                        && !winning_finding
                            .invalidated_hypotheses
                            .contains(&other_f.primary_hypothesis)
                    {
                        winning_finding
                            .invalidated_hypotheses
                            .push(other_f.primary_hypothesis.clone());
                    }
                }
            }
        }
        self.state_store
            .adjudicate_candidate_findings(run_id, winning_candidate_id)?;

        let latest_checkpoint = self
            .state_store
            .get_latest_checkpoint(run_id)?
            .ok_or_else(|| {
                RcaError::Session(format!("No checkpoints found for run {}", run_id))
            })?;

        let mut context: PipelineContext =
            serde_json::from_str(&latest_checkpoint.checkpoint_data_json)?;
        context.vault_path = vault_path.to_string_lossy().to_string();

        let packet: InvestigationContextPacket = match self.state_store.get_context_packet(run_id)? {
            Some(pkt_json) => serde_json::from_str(&pkt_json)?,
            None => self.assemble_context_packet(&context, vault_path)?,
        };

        let report = CurationGate::validate(&packet, &winning_finding, vault_path)?;
        if report.verdict == CurationVerdict::Rejected {
            let err_msg = report
                .issues
                .iter()
                .map(|i| i.message.clone())
                .collect::<Vec<_>>()
                .join("; ");
            return Err(RcaError::Curation(format!(
                "Curated finding rejected by CurationGate: {}",
                err_msg
            )));
        }

        self.apply_crystallization(&mut context, &report, winning_finding.outcome, vault_path)?;

        Ok((context, report))
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
                let mut planned_changes = Vec::new();
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
                                        failed_attempts.push(format!("CL {} ({}) abandoned: {}", cl_number, change_id, reason));

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

                                        let old_content = std::fs::read_to_string(&path)?;
                                        let new_content = artifact.rendered_markdown;
                                        let rel_path = path.strip_prefix(vault_path).unwrap_or(&path).to_path_buf();
                                        let rel_path_str = rel_path.to_string_lossy().to_string();
                                        let diff = generate_unified_diff(&rel_path_str, &old_content, &new_content);
                                        planned_changes.push(PlannedFileChange {
                                            path: rel_path_str,
                                            action: ChangeAction::Modify,
                                            diff,
                                            before_sha256: sha256_hex(old_content.as_bytes()),
                                            after_sha256: sha256_hex(new_content.as_bytes()),
                                            new_content: Some(new_content),
                                        });
                                    }
                                }
                            }
                        }
                    }
                }

                if !planned_changes.is_empty() {
                    let plan = MigrationPlan {
                        plan_version: "1.0.0".to_string(),
                        vault_root: vault_path.to_string_lossy().to_string(),
                        changes: planned_changes,
                    };
                    apply_plan_with_validator(vault_path, &plan, |preview_graph| {
                        let report = akashic_lint::lint_vault(preview_graph);
                        if !report.valid {
                            return Err(akashic_vault::VaultError::Migration(format!(
                                "Pre-apply lint gate failed with {} error(s)",
                                report.error_count
                            )));
                        }
                        Ok(())
                    })?;
                }

                // Auto-requeue follow-up investigation in AWAITING_REASONING and relink storm signature
                let runs = self.state_store.list_workflow_runs().unwrap_or_default();
                let original_run = runs.into_iter().rev().find(|r| {
                    if format!("INC-{}", r.jira_key) == *incident_id {
                        return true;
                    }
                    if let Ok(ctx) = serde_json::from_str::<PipelineContext>(&r.context_json) {
                        ctx.incident_node_id.as_deref() == Some(incident_id)
                    } else {
                        false
                    }
                });

                if let Some(orig) = original_run {
                    if let Ok(mut new_context) = serde_json::from_str::<PipelineContext>(&orig.context_json) {
                        let new_run_id = format!("{}-retry-{}", orig.run_id, cl_number);
                        new_context.run_id = new_run_id.clone();
                        new_context.prior_resolution_id = Some(resolution_id.clone());

                        let mut refreshed_packet = self.assemble_context_packet(&new_context, vault_path)?;
                        let attempt_entry = format!("CL {} ({}) abandoned: {}", cl_number, change_id, reason);
                        if !refreshed_packet.historical_failed_attempts.contains(&attempt_entry) {
                            refreshed_packet.historical_failed_attempts.push(attempt_entry);
                        }
                        if !refreshed_packet.prior_resolutions.iter().any(|p| p.resolution_id == resolution_id) {
                            refreshed_packet.prior_resolutions.push(PriorResolutionSummary {
                                resolution_id: resolution_id.clone(),
                                status: "abandoned".to_string(),
                                remediation_type: None,
                                abandoned_reason: Some(reason.clone()),
                            });
                        }

                        let refreshed_json = serde_json::to_string(&refreshed_packet)?;
                        let new_ctx_json = serde_json::to_string(&new_context)?;
                        let now = current_timestamp_secs();

                        let new_run_record = WorkflowRunRecord {
                            run_id: new_run_id.clone(),
                            jira_key: new_context.jira_key.clone(),
                            current_stage: "AWAITING_REASONING".to_string(),
                            status: "AWAITING_REASONING".to_string(),
                            crash_signature: new_context.crash_signature.clone(),
                            context_json: new_ctx_json.clone(),
                            context_packet_json: Some(refreshed_json.clone()),
                            created_at: now,
                            updated_at: now,
                        };
                        self.state_store.create_workflow_run(&new_run_record)?;
                        self.state_store.save_context_packet(&new_run_id, &refreshed_json)?;
                        self.state_store.save_checkpoint(&new_run_id, "AWAITING_REASONING", &new_ctx_json)?;

                        if let Some(ref sig) = new_context.crash_signature {
                            let _ = self.state_store.update_storm_active_run(sig, &new_run_id);
                        }
                    }
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

        // Stage 5: Crystallize & Lint Gate (includes Stage 6 finalization via apply_crystallization)
        self.run_stage5(&mut context)?;

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
            "RESOLVED" | "DEDUPLICATED" | "INCONCLUSIVE" | "AWAITING_REASONING" | "AWAITING_CURATION" => Ok(context),
            "CRYSTALLIZED" => {
                self.run_stage6(&mut context)?;
                Ok(context)
            }
            "HYPOTHESIS_FALSIFIED" => {
                self.run_stage5(&mut context)?;
                Ok(context)
            }
            "SYMBOLICATED" => {
                self.run_stage4(&mut context).await?;
                self.run_stage5(&mut context)?;
                Ok(context)
            }
            "SANITIZED" => {
                let is_dedup = self.run_stage3(&mut context)?;
                if is_dedup {
                    return Ok(context);
                }
                self.run_stage4(&mut context).await?;
                self.run_stage5(&mut context)?;
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
