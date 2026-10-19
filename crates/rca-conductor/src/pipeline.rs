use crate::db::{current_timestamp_secs, ActiveIncident, SessionRecord, StateStore};
use crate::stage4::{MockStage4Runner, Stage4Input, Stage4Output, Stage4Runner};
use crate::ConductorError;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineInput {
    pub session_id: Option<String>,
    pub jira_key: String,
    pub app_id: String,
    pub device: String,
    pub android_version: String,
    pub raw_logs: String,
    pub vault_path: PathBuf,
    pub alexandria_db: Option<PathBuf>,
    pub prior_resolution_id: Option<String>,
    pub policy_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct PipelineContext {
    pub session_id: String,
    pub jira_key: String,
    pub app_id: String,
    pub device: String,
    pub android_version: String,
    pub raw_logs: String,
    pub vault_path: String,
    pub alexandria_db: Option<String>,
    pub prior_resolution_id: Option<String>,
    pub policy_path: Option<String>,

    pub crash_signature: Option<String>,
    pub deduplicated: bool,
    pub active_incident_id: Option<String>,
    pub sanitized_logs: Option<String>,
    pub redactions_count: usize,
    pub symbolication_output: Option<rca_symbolicator::SymbolicationOutput>,
    pub stage4_output: Option<Stage4Output>,
    pub incident_node_id: Option<String>,
    pub evidence_node_id: Option<String>,
    pub resolution_node_id: Option<String>,
}

pub struct Pipeline<R: Stage4Runner = MockStage4Runner> {
    state_store: StateStore,
    stage4_runner: R,
}

impl<R: Stage4Runner> Pipeline<R> {
    pub fn new(state_store: StateStore, stage4_runner: R) -> Self {
        Self {
            state_store,
            stage4_runner,
        }
    }

    pub fn state_store(&self) -> &StateStore {
        &self.state_store
    }

    pub fn stage4_runner(&self) -> &R {
        &self.stage4_runner
    }

    pub fn run_stage1(&self, context: &mut PipelineContext) -> Result<bool, ConductorError> {
        let triage_symbolicator =
            rca_symbolicator::Symbolicator::new(rca_symbolicator::InMemorySymbolResolver::new());
        let triage_output =
            triage_symbolicator.symbolicate(&context.raw_logs, &context.app_id)?;
        let crash_sig = triage_output.crash_signature;
        context.crash_signature = Some(crash_sig.clone());

        // Check storm gate for 24h window (86400 secs)
        if let Some(existing) = self.state_store.check_storm_gate(&crash_sig, 86400)? {
            context.deduplicated = true;
            context.active_incident_id = Some(existing.incident_id);
            let ctx_json = serde_json::to_string(&context)?;
            self.state_store.update_session(
                &context.session_id,
                "DEDUPLICATED",
                "DEDUPLICATED",
                Some(&crash_sig),
                &ctx_json,
            )?;
            self.state_store
                .save_checkpoint(&context.session_id, "DEDUPLICATED", &ctx_json)?;
            return Ok(true);
        }

        let now = current_timestamp_secs();
        let incident = ActiveIncident {
            incident_id: format!("INC-{}", context.jira_key),
            jira_key: context.jira_key.clone(),
            crash_signature: crash_sig.clone(),
            session_id: context.session_id.clone(),
            created_at: now,
            updated_at: now,
        };
        self.state_store.register_active_incident(&incident)?;
        context.active_incident_id = Some(incident.incident_id);

        let ctx_json = serde_json::to_string(&context)?;
        self.state_store.update_session(
            &context.session_id,
            "TRIAGED",
            "IN_PROGRESS",
            Some(&crash_sig),
            &ctx_json,
        )?;
        self.state_store
            .save_checkpoint(&context.session_id, "TRIAGED", &ctx_json)?;
        Ok(false)
    }

    pub fn run_stage2(&self, context: &mut PipelineContext) -> Result<(), ConductorError> {
        use rca_sanitizer::Sanitizer;

        let sanitizer = rca_sanitizer::FormatPreservingSanitizer::new();
        let sanitized = sanitizer.sanitize(&context.raw_logs)?;
        context.sanitized_logs = Some(sanitized.sanitized_content);
        context.redactions_count = sanitized.redactions_count;

        let ctx_json = serde_json::to_string(&context)?;
        self.state_store.update_session(
            &context.session_id,
            "SANITIZED",
            "IN_PROGRESS",
            context.crash_signature.as_deref(),
            &ctx_json,
        )?;
        self.state_store
            .save_checkpoint(&context.session_id, "SANITIZED", &ctx_json)?;
        Ok(())
    }

    pub fn run_stage3(&self, context: &mut PipelineContext) -> Result<(), ConductorError> {
        let logs_to_symbolicate = context
            .sanitized_logs
            .as_deref()
            .unwrap_or(&context.raw_logs);

        let output = if let Some(ref db_path) = context.alexandria_db {
            let p = Path::new(db_path);
            if p.exists() {
                let store = alexandria_core::store::CodeGraphStore::open(p)?;
                let resolver = rca_symbolicator::AlexandriaSymbolResolver::from_store(store);
                let symbolicator = rca_symbolicator::Symbolicator::new(resolver);
                symbolicator.symbolicate(logs_to_symbolicate, &context.app_id)?
            } else {
                let resolver = rca_symbolicator::InMemorySymbolResolver::new();
                let symbolicator = rca_symbolicator::Symbolicator::new(resolver);
                symbolicator.symbolicate(logs_to_symbolicate, &context.app_id)?
            }
        } else {
            let resolver = rca_symbolicator::InMemorySymbolResolver::new();
            let symbolicator = rca_symbolicator::Symbolicator::new(resolver);
            symbolicator.symbolicate(logs_to_symbolicate, &context.app_id)?
        };

        context.symbolication_output = Some(output);
        let ctx_json = serde_json::to_string(&context)?;
        self.state_store.update_session(
            &context.session_id,
            "SYMBOLICATED",
            "IN_PROGRESS",
            context.crash_signature.as_deref(),
            &ctx_json,
        )?;
        self.state_store
            .save_checkpoint(&context.session_id, "SYMBOLICATED", &ctx_json)?;
        Ok(())
    }

    pub async fn run_stage4(&self, context: &mut PipelineContext) -> Result<(), ConductorError> {
        let stage4_input = Stage4Input {
            session_id: context.session_id.clone(),
            jira_key: context.jira_key.clone(),
            app_id: context.app_id.clone(),
            crash_signature: context.crash_signature.clone().unwrap_or_default(),
            sanitized_logs: context.sanitized_logs.clone().unwrap_or_default(),
            symbolication_evidence: context
                .symbolication_output
                .as_ref()
                .map(|o| o.evidence_payload.clone())
                .unwrap_or(serde_json::Value::Null),
            policy_path: context.policy_path.as_ref().map(PathBuf::from),
        };

        let output = self.stage4_runner.run_stage4(&stage4_input).await?;
        context.stage4_output = Some(output);

        let ctx_json = serde_json::to_string(&context)?;
        self.state_store.update_session(
            &context.session_id,
            "HYPOTHESIS_FALSIFIED",
            "IN_PROGRESS",
            context.crash_signature.as_deref(),
            &ctx_json,
        )?;
        self.state_store
            .save_checkpoint(&context.session_id, "HYPOTHESIS_FALSIFIED", &ctx_json)?;
        Ok(())
    }

    pub fn run_stage5(&self, context: &mut PipelineContext) -> Result<(), ConductorError> {
        let vault_path = PathBuf::from(&context.vault_path);
        let writer = rca_crystallizer::CrystallizerWriter::new(&vault_path);

        let incident_id = format!("INC-{}", context.jira_key);
        let evidence_id = format!("EVD-{}-STACK", context.jira_key);

        let mut scope = std::collections::BTreeMap::new();
        scope.insert("app".to_string(), context.app_id.clone());
        if !context.device.is_empty() {
            scope.insert("device".to_string(), context.device.clone());
        }
        if !context.android_version.is_empty() {
            scope.insert("android_version".to_string(), context.android_version.clone());
        }

        let incident_node = rca_crystallizer::IncidentNode {
            id: incident_id.clone(),
            title: format!(
                "Crash in {}: {}",
                context.app_id,
                context
                    .stage4_output
                    .as_ref()
                    .map(|o| o.hypothesis.as_str())
                    .unwrap_or("Incident investigation")
            ),
            status: "active".to_string(),
            valid_from: Some("2026-10-19".to_string()),
            crash_signature: context.crash_signature.clone().unwrap_or_default(),
            scope: scope.clone(),
            external_refs: Some(rca_crystallizer::ExternalRefs {
                jira_key: Some(context.jira_key.clone()),
                bug2go_id: None,
            }),
            relations: vec![rca_crystallizer::CrystallizerRelation {
                relation_type: "supported_by".to_string(),
                target: format!("../evidence/{}.md", evidence_id),
                evidence: None,
            }],
            body: format!(
                "### Hypothesis\n{}\n\n### Falsification Results\n{}\n\n### Remediation Proposal\n{}\n",
                context
                    .stage4_output
                    .as_ref()
                    .map(|o| o.hypothesis.as_str())
                    .unwrap_or("None"),
                context
                    .stage4_output
                    .as_ref()
                    .map(|o| o.falsification_results.join("\n"))
                    .unwrap_or_default(),
                context
                    .stage4_output
                    .as_ref()
                    .map(|o| o.remediation_proposal.as_str())
                    .unwrap_or("None")
            ),
        };

        let stack_body = context
            .symbolication_output
            .as_ref()
            .map(|o| format!("```\n{}\n```", o.symbolicated_stack_trace))
            .unwrap_or_else(|| "```\nNo stack trace available\n```".to_string());

        let evidence_node = rca_crystallizer::EvidenceNode {
            id: evidence_id.clone(),
            title: format!("Symbolicated stack trace for {}", context.jira_key),
            status: "active".to_string(),
            scope: scope.clone(),
            external_refs: Some(rca_crystallizer::ExternalRefs {
                jira_key: Some(context.jira_key.clone()),
                bug2go_id: None,
            }),
            relations: vec![rca_crystallizer::CrystallizerRelation {
                relation_type: "substantiates".to_string(),
                target: format!("../incidents/{}.md", incident_id),
                evidence: None,
            }],
            body: stack_body,
        };

        writer.write_incident(&incident_node)?;
        writer.write_evidence(&evidence_node)?;

        let report = rca_crystallizer::validate_vault(&vault_path)?;
        if report.errors > 0 || !report.healthy {
            return Err(ConductorError::VaultValidationFailed(
                report.errors,
                report.warnings,
            ));
        }

        context.incident_node_id = Some(incident_id);
        context.evidence_node_id = Some(evidence_id);

        let ctx_json = serde_json::to_string(&context)?;
        self.state_store.update_session(
            &context.session_id,
            "CRYSTALLIZED",
            "IN_PROGRESS",
            context.crash_signature.as_deref(),
            &ctx_json,
        )?;
        self.state_store
            .save_checkpoint(&context.session_id, "CRYSTALLIZED", &ctx_json)?;
        Ok(())
    }

    pub fn run_stage6(&self, context: &mut PipelineContext) -> Result<(), ConductorError> {
        let vault_path = PathBuf::from(&context.vault_path);
        let writer = rca_crystallizer::CrystallizerWriter::new(&vault_path);

        let resolution_id = format!("RES-{}-01", context.jira_key);
        let incident_target = format!(
            "../incidents/{}.md",
            context
                .incident_node_id
                .as_deref()
                .unwrap_or(&format!("INC-{}", context.jira_key))
        );

        let mut relations = vec![rca_crystallizer::CrystallizerRelation {
            relation_type: "resolves".to_string(),
            target: incident_target,
            evidence: None,
        }];

        if let Some(ref prior_id) = context.prior_resolution_id {
            let target = if prior_id.ends_with(".md") {
                format!("../resolutions/{}", prior_id)
            } else {
                format!("../resolutions/{}.md", prior_id)
            };
            relations.push(rca_crystallizer::CrystallizerRelation {
                relation_type: "supersedes".to_string(),
                target,
                evidence: None,
            });
        }

        let resolution_node = rca_crystallizer::ResolutionNode {
            id: resolution_id.clone(),
            title: format!("Resolution for {}", context.jira_key),
            status: "active".to_string(),
            remediation_type: "permanent_fix".to_string(),
            abandoned_reason: None,
            external_refs: Some(rca_crystallizer::ResolutionExternalRefs {
                jira_key: Some(context.jira_key.clone()),
                gerrit_cls: vec![],
            }),
            relations,
            body: format!(
                "### Remediation Plan\n{}\n",
                context
                    .stage4_output
                    .as_ref()
                    .map(|o| o.remediation_proposal.as_str())
                    .unwrap_or("None")
            ),
        };

        writer.write_resolution(&resolution_node)?;

        let report = rca_crystallizer::validate_vault(&vault_path)?;
        if report.errors > 0 || !report.healthy {
            return Err(ConductorError::VaultValidationFailed(
                report.errors,
                report.warnings,
            ));
        }

        context.resolution_node_id = Some(resolution_id);

        let ctx_json = serde_json::to_string(&context)?;
        self.state_store.update_session(
            &context.session_id,
            "RESOLVED",
            "COMPLETED",
            context.crash_signature.as_deref(),
            &ctx_json,
        )?;
        self.state_store
            .save_checkpoint(&context.session_id, "RESOLVED", &ctx_json)?;
        Ok(())
    }

    pub async fn run(&self, input: PipelineInput) -> Result<PipelineContext, ConductorError> {
        let session_id = input.session_id.clone().unwrap_or_else(|| {
            format!(
                "sess-{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            )
        });

        let mut context = PipelineContext {
            session_id: session_id.clone(),
            jira_key: input.jira_key.clone(),
            app_id: input.app_id.clone(),
            device: input.device.clone(),
            android_version: input.android_version.clone(),
            raw_logs: input.raw_logs.clone(),
            vault_path: input.vault_path.to_string_lossy().to_string(),
            alexandria_db: input.alexandria_db.map(|p| p.to_string_lossy().to_string()),
            prior_resolution_id: input.prior_resolution_id,
            policy_path: input.policy_path.map(|p| p.to_string_lossy().to_string()),
            ..PipelineContext::default()
        };

        let now = current_timestamp_secs();
        let initial_session = SessionRecord {
            session_id: session_id.clone(),
            jira_key: input.jira_key.clone(),
            crash_signature: None,
            stage: "CREATED".to_string(),
            status: "IN_PROGRESS".to_string(),
            created_at: now,
            updated_at: now,
            context_json: serde_json::to_string(&context)?,
        };
        self.state_store.create_session(&initial_session)?;

        // Stage 1: Triage & Storm Gate
        let is_dedup = self.run_stage1(&mut context)?;
        if is_dedup {
            return Ok(context);
        }

        // Stage 2: Log Sanitization
        self.run_stage2(&mut context)?;

        // Stage 3: Native & Java Symbolication
        self.run_stage3(&mut context)?;

        // Stage 4: Hypothesis Generation & Adversarial Falsification
        self.run_stage4(&mut context).await?;

        // Stage 5: Akashic Crystallization & Lint Gate
        self.run_stage5(&mut context)?;

        // Stage 6: Resolution Tracking & Forward Supersession
        self.run_stage6(&mut context)?;

        Ok(context)
    }

    pub async fn resume_session(
        &self,
        session_id: &str,
        vault_path_override: Option<&Path>,
        alexandria_db_override: Option<&Path>,
    ) -> Result<PipelineContext, ConductorError> {
        let latest_checkpoint = self
            .state_store
            .get_latest_checkpoint(session_id)?
            .ok_or_else(|| {
                ConductorError::Session(format!("No checkpoints found for session {}", session_id))
            })?;

        let mut context: PipelineContext = serde_json::from_str(&latest_checkpoint.state_json)?;
        if let Some(vp) = vault_path_override {
            context.vault_path = vp.to_string_lossy().to_string();
        }
        if let Some(adb) = alexandria_db_override {
            context.alexandria_db = Some(adb.to_string_lossy().to_string());
        }

        match latest_checkpoint.stage.as_str() {
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
                self.run_stage3(&mut context)?;
                self.run_stage4(&mut context).await?;
                self.run_stage5(&mut context)?;
                self.run_stage6(&mut context)?;
                Ok(context)
            }
            "TRIAGED" => {
                self.run_stage2(&mut context)?;
                self.run_stage3(&mut context)?;
                self.run_stage4(&mut context).await?;
                self.run_stage5(&mut context)?;
                self.run_stage6(&mut context)?;
                Ok(context)
            }
            "CREATED" => {
                let is_dedup = self.run_stage1(&mut context)?;
                if is_dedup {
                    return Ok(context);
                }
                self.run_stage2(&mut context)?;
                self.run_stage3(&mut context)?;
                self.run_stage4(&mut context).await?;
                self.run_stage5(&mut context)?;
                self.run_stage6(&mut context)?;
                Ok(context)
            }
            other => Err(ConductorError::StageFailed(format!(
                "Unknown checkpoint stage for resumption: {}",
                other
            ))),
        }
    }
}

impl Pipeline<MockStage4Runner> {
    pub fn with_mock(state_store: StateStore) -> Self {
        Self::new(state_store, MockStage4Runner::new())
    }
}
