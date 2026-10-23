use crate::stage4::{CandidateFinding, FindingOutcome};
use crate::symbolicator::SymbolicationResult;
use crate::RcaError;
use akashic_lint::LintIssue;
use akashic_vault::crystallizer::{
    build_evidence_node, build_incident_node, build_resolution_node, EvidenceInput, IncidentInput,
    ResolutionInput,
};
use akashic_vault::frontmatter::{NodeStatus, Relation, RelationType, RemediationType, Scope};
use akashic_vault::migrator::{
    generate_unified_diff, sha256_hex, ChangeAction, MigrationPlan, PlannedFileChange,
};
use akashic_vault::parser::serialize_document;
use akashic_vault::VaultGraph;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlastRadiusContext {
    pub repo_name: String,
    pub seed_symbol_ids: Vec<String>,
    pub upstream_callers: Vec<String>,
    pub downstream_dependencies: Vec<String>,
    pub impacted_communities: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PriorResolutionSummary {
    pub resolution_id: String,
    pub status: String,
    pub remediation_type: Option<String>,
    pub abandoned_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvestigationContextPacket {
    pub run_id: String,
    pub incident_id: String,
    pub jira_key: String,
    pub app_id: String,
    pub device: String,
    pub android_version: String,
    pub crash_signature: String,
    pub sanitized_logs: String,
    pub redactions_count: usize,
    pub symbolication: SymbolicationResult,
    pub blast_radius: Option<BlastRadiusContext>,
    #[serde(default)]
    pub historical_invalidated_hypotheses: Vec<String>,
    #[serde(default)]
    pub historical_failed_attempts: Vec<String>,
    #[serde(default)]
    pub prior_resolutions: Vec<PriorResolutionSummary>,
    #[serde(default)]
    pub policy_path: Option<PathBuf>,
}

impl InvestigationContextPacket {
    pub fn resolved_symbol_ids(&self) -> Vec<String> {
        let mut ids = Vec::new();
        if let Some(ref sym) = self.symbolication.faulting_symbol {
            if !sym.symbol.is_empty() {
                ids.push(sym.symbol.clone());
            }
        }
        for f in &self.symbolication.frames {
            if let Some(ref sym) = f.raw_symbol {
                if !sym.is_empty() && !ids.contains(sym) {
                    ids.push(sym.clone());
                }
            } else if let Some(ref method) = f.method_name {
                if !method.is_empty() && !ids.contains(method) {
                    ids.push(method.clone());
                }
            }
        }
        ids
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CurationVerdict {
    Approved,
    Rejected,
    HeldForCuration,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CurationIssue {
    pub invariant: String,
    pub severity: String, // "error" | "warning"
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurationReport {
    pub verdict: CurationVerdict,
    pub issues: Vec<CurationIssue>,
    pub incident_node_id: Option<String>,
    pub evidence_node_id: Option<String>,
    pub resolution_node_id: Option<String>,
    #[serde(skip)]
    pub migration_plan: Option<MigrationPlan>,
    #[serde(skip)]
    pub lint_issues: Vec<LintIssue>,
}

fn normalize_for_comparison(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

pub struct CurationGate;

impl CurationGate {
    pub fn validate(
        packet: &InvestigationContextPacket,
        finding: &CandidateFinding,
        vault_path: &Path,
    ) -> Result<CurationReport, RcaError> {
        let mut issues = Vec::new();
        let mut lint_issues = Vec::new();

        // -------------------------------------------------------------
        // Invariant 1: C001 (Negative-Knowledge Non-Regression)
        // -------------------------------------------------------------
        let norm_hyp = normalize_for_comparison(&finding.primary_hypothesis);
        let norm_fix = normalize_for_comparison(&finding.remediation_proposal);

        for historical in &packet.historical_invalidated_hypotheses {
            let norm_hist = normalize_for_comparison(historical);
            if !norm_hist.is_empty() {
                if norm_hyp == norm_hist || norm_hyp.contains(&norm_hist) {
                    issues.push(CurationIssue {
                        invariant: "C001".to_string(),
                        severity: "error".to_string(),
                        message: format!(
                            "Candidate hypothesis matches previously invalidated hypothesis: '{}'",
                            historical
                        ),
                    });
                }
                if finding.outcome == FindingOutcome::Resolved
                    && (norm_fix == norm_hist || norm_fix.contains(&norm_hist))
                {
                    issues.push(CurationIssue {
                        invariant: "C001".to_string(),
                        severity: "error".to_string(),
                        message: format!(
                            "Remediation proposal matches previously invalidated hypothesis: '{}'",
                            historical
                        ),
                    });
                }
            }
        }

        for failed in &packet.historical_failed_attempts {
            let norm_failed = normalize_for_comparison(failed);
            if !norm_failed.is_empty() {
                if norm_hyp == norm_failed || norm_hyp.contains(&norm_failed) {
                    issues.push(CurationIssue {
                        invariant: "C001".to_string(),
                        severity: "error".to_string(),
                        message: format!(
                            "Candidate hypothesis matches previously failed attempt: '{}'",
                            failed
                        ),
                    });
                }
                if finding.outcome == FindingOutcome::Resolved
                    && (norm_fix == norm_failed || norm_fix.contains(&norm_failed))
                {
                    issues.push(CurationIssue {
                        invariant: "C001".to_string(),
                        severity: "error".to_string(),
                        message: format!(
                            "Remediation proposal matches previously failed attempt: '{}'",
                            failed
                        ),
                    });
                }
            }
        }

        // -------------------------------------------------------------
        // Invariant 2: C002 (Falsification Trace Completeness)
        // -------------------------------------------------------------
        match finding.outcome {
            FindingOutcome::Resolved => {
                if finding.primary_hypothesis.trim().is_empty() {
                    issues.push(CurationIssue {
                        invariant: "C002".to_string(),
                        severity: "error".to_string(),
                        message: "primary_hypothesis cannot be empty for resolved finding".to_string(),
                    });
                }
                if finding.remediation_proposal.trim().is_empty() {
                    issues.push(CurationIssue {
                        invariant: "C002".to_string(),
                        severity: "error".to_string(),
                        message: "remediation_proposal cannot be empty for resolved finding".to_string(),
                    });
                }
                if finding.falsification_attempts.is_empty() {
                    issues.push(CurationIssue {
                        invariant: "C002".to_string(),
                        severity: "error".to_string(),
                        message: "falsification_attempts cannot be empty for resolved finding".to_string(),
                    });
                }
            }
            FindingOutcome::Inconclusive => {
                if finding.invalidated_hypotheses.is_empty() {
                    issues.push(CurationIssue {
                        invariant: "C002".to_string(),
                        severity: "error".to_string(),
                        message: "Inconclusive finding must record at least one invalidated hypothesis".to_string(),
                    });
                }
            }
        }

        // -------------------------------------------------------------
        // Invariant 3: C003 (Empirical Grounding Check)
        // -------------------------------------------------------------
        if let Some(ref faulting) = packet.symbolication.faulting_symbol {
            let mut grounded = false;
            let faulting_terms = [
                faulting.symbol.as_str(),
                faulting.class_name.as_deref().unwrap_or(""),
                faulting.file.as_deref().unwrap_or(""),
            ];
            let corpus = format!(
                "{} {} {}",
                finding.primary_hypothesis,
                finding.falsification_attempts.join(" "),
                finding.remediation_proposal
            )
            .to_lowercase();

            for term in faulting_terms {
                let t = term.trim().to_lowercase();
                if !t.is_empty() && corpus.contains(&t) {
                    grounded = true;
                    break;
                }
            }
            if finding.affected_symbols.iter().any(|s| s == &faulting.symbol) {
                grounded = true;
            }

            if !grounded && (!faulting.symbol.is_empty() || faulting.file.is_some()) {
                issues.push(CurationIssue {
                    invariant: "C003".to_string(),
                    severity: "warning".to_string(),
                    message: format!(
                        "Finding does not reference resolved faulting symbol '{}' or file '{:?}'",
                        faulting.symbol, faulting.file
                    ),
                });
            }
        }

        // -------------------------------------------------------------
        // Invariant 4: C004 (In-Memory MigrationPlan Preview, Lint & Scope Conflict Gate)
        // -------------------------------------------------------------
        let incident_id = packet.incident_id.clone();
        let evidence_id = format!("EVD-{}-STACK", packet.jira_key);
        let resolution_id = format!("RES-{}-01", packet.jira_key);

        let mut scope = Scope::new();
        scope.insert("app".to_string(), packet.app_id.clone());
        if !packet.device.is_empty() {
            scope.insert("device".to_string(), packet.device.clone());
        }
        if !packet.android_version.is_empty() {
            scope.insert("android_version".to_string(), packet.android_version.clone());
        }
        if let Some(ref fault) = packet.symbolication.faulting_symbol {
            if !fault.repo_name.is_empty() {
                scope.insert("repo".to_string(), fault.repo_name.clone());
            }
        }

        // 1. INCIDENT node artifact
        let inc_body = format!(
            "### Hypothesis\n{}\n\n### Falsification Attempts\n{}\n\n### Remediation Proposal\n{}\n",
            finding.primary_hypothesis,
            finding.falsification_attempts.join("\n"),
            finding.remediation_proposal
        );
        let mut inc_artifact = build_incident_node(IncidentInput {
            id: incident_id.clone(),
            title: format!("Crash in {}: {}", packet.app_id, finding.primary_hypothesis),
            crash_signature: packet.crash_signature.clone(),
            status: Some(NodeStatus::Active),
            scope: scope.clone(),
            external_refs: None,
            relations: vec![Relation {
                target: evidence_id.clone(),
                relation_type: RelationType::SupportedBy,
                evidence: None,
                scope: scope.clone(),
            }],
            body: inc_body,
        })?;
        inc_artifact.frontmatter.valid_from = Some("2026-10-19".to_string());
        inc_artifact.frontmatter.invalidated_hypotheses = finding.invalidated_hypotheses.clone();
        inc_artifact.frontmatter.failed_attempts = finding.failed_attempts.clone();
        inc_artifact.rendered_markdown =
            serialize_document(&inc_artifact.frontmatter, &inc_artifact.body)?;

        // 2. EVIDENCE node artifact
        let stack_body = format!(
            "```\n{}\n```",
            packet.symbolication.symbolicated_stack_trace
        );
        let mut evd_artifact = build_evidence_node(EvidenceInput {
            id: evidence_id.clone(),
            title: format!("Symbolicated stack trace for {}", packet.jira_key),
            incident_id: Some(incident_id.clone()),
            status: Some(NodeStatus::Active),
            scope: scope.clone(),
            invalidated_hypotheses: finding.invalidated_hypotheses.clone(),
            failed_attempts: finding.failed_attempts.clone(),
            relations: vec![Relation {
                target: incident_id.clone(),
                relation_type: RelationType::Substantiates,
                evidence: None,
                scope: scope.clone(),
            }],
            body: stack_body,
        })?;
        evd_artifact.frontmatter.valid_from = Some("2026-10-19".to_string());
        evd_artifact.rendered_markdown =
            serialize_document(&evd_artifact.frontmatter, &evd_artifact.body)?;

        // 3. RESOLUTION node artifact (when outcome == Resolved)
        let prior_res_id = finding
            .supersedes_resolution_id
            .as_ref()
            .cloned()
            .or_else(|| packet.prior_resolutions.first().map(|p| p.resolution_id.clone()));

        let res_artifact_opt = if finding.outcome == FindingOutcome::Resolved {
            let mut res_relations = vec![Relation {
                target: incident_id.clone(),
                relation_type: RelationType::Resolves,
                evidence: Some(evidence_id.clone()),
                scope: scope.clone(),
            }];

            if let Some(ref prior_id) = prior_res_id {
                res_relations.push(Relation {
                    target: prior_id.clone(),
                    relation_type: RelationType::Supersedes,
                    evidence: Some(evidence_id.clone()),
                    scope: scope.clone(),
                });
            }

            let rem_type = match finding.remediation_type.as_deref() {
                Some("hotfix") => RemediationType::Hotfix,
                Some("workaround") => RemediationType::Workaround,
                Some("revert") => RemediationType::Revert,
                _ => RemediationType::PermanentFix,
            };

            let res_body = format!("### Remediation Plan\n{}\n", finding.remediation_proposal);
            let mut res_artifact = build_resolution_node(ResolutionInput {
                id: resolution_id.clone(),
                title: format!("Resolution for {}", packet.jira_key),
                incident_id: Some(incident_id.clone()),
                remediation_type: rem_type,
                status: Some(NodeStatus::Active),
                abandoned_reason: None,
                scope: scope.clone(),
                external_refs: None,
                relations: res_relations,
                body: res_body,
            })?;
            res_artifact.frontmatter.valid_from = Some("2026-10-19".to_string());
            res_artifact.frontmatter.invalidated_hypotheses = finding.invalidated_hypotheses.clone();
            res_artifact.frontmatter.failed_attempts = finding.failed_attempts.clone();
            res_artifact.rendered_markdown =
                serialize_document(&res_artifact.frontmatter, &res_artifact.body)?;
            Some(res_artifact)
        } else {
            None
        };

        // Construct PlannedFileChange entries
        let make_planned_change = |rel_path: &str, new_content: String| -> PlannedFileChange {
            let full = vault_path.join(rel_path);
            let (action, before_sha256, diff) = if full.exists() {
                let existing = std::fs::read_to_string(&full).unwrap_or_default();
                let before_hash = sha256_hex(existing.as_bytes());
                let d = generate_unified_diff(rel_path, &existing, &new_content);
                (ChangeAction::Modify, before_hash, d)
            } else {
                let before_hash = sha256_hex(b"");
                let d = generate_unified_diff(rel_path, "", &new_content);
                (ChangeAction::Create, before_hash, d)
            };
            let after_sha256 = sha256_hex(new_content.as_bytes());
            PlannedFileChange {
                path: rel_path.to_string(),
                before_sha256,
                after_sha256,
                action,
                diff,
                new_content: Some(new_content),
            }
        };

        let mut changes = vec![
            make_planned_change(
                &format!("incidents/{}.md", incident_id),
                inc_artifact.rendered_markdown,
            ),
            make_planned_change(
                &format!("evidence/{}.md", evidence_id),
                evd_artifact.rendered_markdown,
            ),
        ];

        if let Some(ref res_art) = res_artifact_opt {
            changes.push(make_planned_change(
                &format!("resolutions/{}.md", resolution_id),
                res_art.rendered_markdown.clone(),
            ));
        }

        // If a prior resolution is superseded, update its status in the migration plan
        if let Some(ref prior_id) = prior_res_id {
            let prior_rel = format!("resolutions/{}.md", prior_id);
            let prior_full = vault_path.join(&prior_rel);
            if prior_full.exists() {
                if let Ok(prior_doc) = akashic_vault::parser::parse_markdown_file(vault_path, &prior_full) {
                    let mut updated_fm = prior_doc.frontmatter;
                    updated_fm.status = NodeStatus::Superseded;
                    if let Ok(rendered_prior) = serialize_document(&updated_fm, &prior_doc.body) {
                        changes.push(make_planned_change(&prior_rel, rendered_prior));
                    }
                }
            }
        }

        let plan = MigrationPlan {
            plan_version: "1.0.0".to_string(),
            vault_root: vault_path.to_string_lossy().to_string(),
            changes,
        };

        let mut has_unresolved_scope_conflict = false;

        // Preview in-memory graph
        if vault_path.exists() {
            match VaultGraph::from_dir_with_plan(vault_path, &plan) {
                Ok(preview_graph) => {
                    let lint_report = akashic_lint::lint_vault(&preview_graph);
                    if !lint_report.valid {
                        for issue in &lint_report.issues {
                            if issue.severity == akashic_lint::Severity::Error {
                                issues.push(CurationIssue {
                                    invariant: "C004".to_string(),
                                    severity: "error".to_string(),
                                    message: format!("Vault lint error ({}): {}", issue.code, issue.message),
                                });
                            }
                        }
                        lint_issues = lint_report.issues;
                    }

                    let cycles = preview_graph.find_cycles(&[RelationType::Supersedes]);
                    if !cycles.is_empty() {
                        issues.push(CurationIssue {
                            invariant: "C004".to_string(),
                            severity: "error".to_string(),
                            message: format!("Supersession cycle detected: {:?}", cycles),
                        });
                    }

                    if let Ok(curr_packet) =
                        akashic_vault::query::query_current(&preview_graph, &incident_id, Some(&scope))
                    {
                        if let Some(ref conflict) = curr_packet.unresolved_conflict {
                            issues.push(CurationIssue {
                                invariant: "C004".to_string(),
                                severity: "warning".to_string(),
                                message: format!("Unresolved scope conflict: {}", conflict.message),
                            });
                            has_unresolved_scope_conflict = true;
                        }
                    }
                }
                Err(e) => {
                    issues.push(CurationIssue {
                        invariant: "C004".to_string(),
                        severity: "error".to_string(),
                        message: format!("Failed to preview migration plan in vault: {}", e),
                    });
                }
            }
        }

        let has_errors = issues.iter().any(|i| i.severity == "error");
        let verdict = if has_errors {
            CurationVerdict::Rejected
        } else if finding.hold_for_curation || has_unresolved_scope_conflict {
            CurationVerdict::HeldForCuration
        } else {
            CurationVerdict::Approved
        };

        let res_id_opt = if finding.outcome == FindingOutcome::Resolved {
            Some(resolution_id)
        } else {
            None
        };

        Ok(CurationReport {
            verdict,
            issues,
            incident_node_id: Some(incident_id),
            evidence_node_id: Some(evidence_id),
            resolution_node_id: res_id_opt,
            migration_plan: Some(plan),
            lint_issues,
        })
    }
}
