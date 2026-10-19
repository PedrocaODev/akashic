use serde::{Deserialize, Serialize};

pub const SELF_TRIGGER_PREFIX: &str = "akashic/";
pub const SELF_TRIGGER_ALT_PREFIX: &str = "akashic-";
pub const CRYSTALLIZE_COMMIT_PREFIX: &str = "[akashic-crystallize]";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GerritEventType {
    ChangeMerged,
    PatchsetCreated,
    ChangeAbandoned,
    CommentAdded,
    RefUpdated,
    CatchupChange,
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GerritStreamEvent {
    pub event_type: String,
    pub project: Option<String>,
    pub branch: Option<String>,
    pub topic: Option<String>,
    pub change_id: Option<String>,
    pub change_number: Option<i64>,
    pub revision: Option<String>,
    pub commit_message: Option<String>,
    pub comment: Option<String>,
    pub event_created_on: Option<i64>,
    pub raw_json: serde_json::Value,
}

impl GerritStreamEvent {
    pub fn kind(&self) -> GerritEventType {
        match self.event_type.as_str() {
            "change-merged" => GerritEventType::ChangeMerged,
            "patchset-created" => GerritEventType::PatchsetCreated,
            "change-abandoned" => GerritEventType::ChangeAbandoned,
            "comment-added" => GerritEventType::CommentAdded,
            "ref-updated" => GerritEventType::RefUpdated,
            "catchup-change" => GerritEventType::CatchupChange,
            other => GerritEventType::Unknown(other.to_string()),
        }
    }

    pub fn is_supported_event(&self) -> bool {
        !matches!(self.kind(), GerritEventType::Unknown(_))
    }

    pub fn is_self_triggered(&self) -> bool {
        is_self_triggered_event(self, None)
    }
}

fn extract_str(val: &serde_json::Value, paths: &[&[&str]]) -> Option<String> {
    for path in paths {
        let mut curr = val;
        let mut found = true;
        for key in *path {
            if let Some(next) = curr.get(*key) {
                curr = next;
            } else {
                found = false;
                break;
            }
        }
        if found {
            if let Some(s) = curr.as_str() {
                return Some(s.to_string());
            }
        }
    }
    None
}

fn extract_i64(val: &serde_json::Value, paths: &[&[&str]]) -> Option<i64> {
    for path in paths {
        let mut curr = val;
        let mut found = true;
        for key in *path {
            if let Some(next) = curr.get(*key) {
                curr = next;
            } else {
                found = false;
                break;
            }
        }
        if found {
            if let Some(n) = curr.as_i64() {
                return Some(n);
            }
            if let Some(s) = curr.as_str() {
                if let Ok(n) = s.parse::<i64>() {
                    return Some(n);
                }
            }
        }
    }
    None
}

pub fn parse_stream_event(line: &str) -> Result<GerritStreamEvent, serde_json::Error> {
    let raw: serde_json::Value = serde_json::from_str(line)?;

    let mut event_type = extract_str(&raw, &[&["type"]]).unwrap_or_default();

    let project = extract_str(
        &raw,
        &[
            &["change", "project"],
            &["project"],
            &["refUpdate", "project"],
        ],
    );

    let branch = extract_str(
        &raw,
        &[
            &["change", "branch"],
            &["branch"],
            &["refUpdate", "refName"],
        ],
    );

    let topic = extract_str(&raw, &[&["change", "topic"], &["topic"]]);

    let change_id = extract_str(&raw, &[&["change", "id"], &["id"], &["changeKey", "id"]]);

    let change_number = extract_i64(&raw, &[&["change", "number"], &["number"]]);

    let revision = extract_str(
        &raw,
        &[
            &["patchSet", "revision"],
            &["currentPatchSet", "revision"],
            &["newRev"],
            &["revision"],
        ],
    );

    let commit_message = extract_str(
        &raw,
        &[
            &["change", "commitMessage"],
            &["commitMessage"],
            &["patchSet", "commitMessage"],
        ],
    );

    let comment = extract_str(&raw, &[&["comment"], &["comment", "message"]]);

    let event_created_on = extract_i64(
        &raw,
        &[&["eventCreatedOn"], &["lastUpdated"], &["createdOn"]],
    );

    if event_type.is_empty() && project.is_some() && (change_id.is_some() || change_number.is_some()) {
        event_type = "catchup-change".to_string();
    }

    Ok(GerritStreamEvent {
        event_type,
        project,
        branch,
        topic,
        change_id,
        change_number,
        revision,
        commit_message,
        comment,
        event_created_on,
        raw_json: raw,
    })
}

fn starts_with_prefix(value: &str, custom_prefix: Option<&str>) -> bool {
    if value.starts_with(SELF_TRIGGER_PREFIX) || value.starts_with(SELF_TRIGGER_ALT_PREFIX) {
        return true;
    }
    if let Some(cp) = custom_prefix {
        if !cp.is_empty() && value.starts_with(cp) {
            return true;
        }
    }
    false
}

pub fn is_self_triggered_event(event: &GerritStreamEvent, custom_prefix: Option<&str>) -> bool {
    if let Some(ref project) = event.project {
        if starts_with_prefix(project, custom_prefix) {
            return true;
        }
    }

    if let Some(ref branch) = event.branch {
        let stripped = branch.strip_prefix("refs/heads/").unwrap_or(branch);
        if starts_with_prefix(branch, custom_prefix) || starts_with_prefix(stripped, custom_prefix) {
            return true;
        }
    }

    if let Some(ref topic) = event.topic {
        if starts_with_prefix(topic, custom_prefix) {
            return true;
        }
    }

    if let Some(ref commit_msg) = event.commit_message {
        let trimmed = commit_msg.trim();
        if trimmed.starts_with(CRYSTALLIZE_COMMIT_PREFIX)
            || trimmed.starts_with(SELF_TRIGGER_PREFIX)
            || trimmed.starts_with(SELF_TRIGGER_ALT_PREFIX)
            || trimmed.contains(CRYSTALLIZE_COMMIT_PREFIX)
        {
            return true;
        }
    }

    if let Some(ref comment) = event.comment {
        if comment.contains(CRYSTALLIZE_COMMIT_PREFIX) {
            return true;
        }
    }

    false
}

pub fn should_process_event(event: &GerritStreamEvent, custom_prefix: Option<&str>) -> bool {
    event.is_supported_event() && !is_self_triggered_event(event, custom_prefix)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_change_merged_event() {
        let json = r#"{
            "type": "change-merged",
            "change": {
                "project": "platform/packages/apps/Settings",
                "branch": "master",
                "topic": "bluetooth-fix",
                "id": "I1234567890abcdef",
                "number": 4201,
                "commitMessage": "Fix bluetooth crash on boot\n\nChange-Id: I1234567890abcdef"
            },
            "newRev": "abc1234567def",
            "eventCreatedOn": 1700000000
        }"#;

        let ev = parse_stream_event(json).expect("valid event");
        assert_eq!(ev.kind(), GerritEventType::ChangeMerged);
        assert_eq!(ev.project.as_deref(), Some("platform/packages/apps/Settings"));
        assert_eq!(ev.branch.as_deref(), Some("master"));
        assert_eq!(ev.topic.as_deref(), Some("bluetooth-fix"));
        assert_eq!(ev.change_id.as_deref(), Some("I1234567890abcdef"));
        assert_eq!(ev.change_number, Some(4201));
        assert_eq!(ev.revision.as_deref(), Some("abc1234567def"));
        assert_eq!(ev.event_created_on, Some(1700000000));
        assert!(ev.is_supported_event());
        assert!(!ev.is_self_triggered());
    }

    #[test]
    fn test_parse_catchup_query_event() {
        let json = r#"{
            "project": "vendor/moto/camera",
            "branch": "android14",
            "id": "Ifeedfacecafe",
            "number": "9999",
            "currentPatchSet": {
                "revision": "feedface0101"
            },
            "lastUpdated": 1700001000
        }"#;

        let ev = parse_stream_event(json).expect("valid catchup change");
        assert_eq!(ev.kind(), GerritEventType::CatchupChange);
        assert_eq!(ev.project.as_deref(), Some("vendor/moto/camera"));
        assert_eq!(ev.change_number, Some(9999));
        assert_eq!(ev.revision.as_deref(), Some("feedface0101"));
        assert_eq!(ev.event_created_on, Some(1700001000));
        assert!(should_process_event(&ev, None));
    }

    #[test]
    fn test_self_trigger_loop_detection() {
        let ev_proj = GerritStreamEvent {
            event_type: "change-merged".to_string(),
            project: Some("akashic/knowledge-base".to_string()),
            branch: Some("master".to_string()),
            topic: None,
            change_id: Some("I111".to_string()),
            change_number: Some(1),
            revision: None,
            commit_message: Some("docs update".to_string()),
            comment: None,
            event_created_on: None,
            raw_json: serde_json::Value::Null,
        };
        assert!(ev_proj.is_self_triggered());
        assert!(!should_process_event(&ev_proj, None));

        let ev_branch = GerritStreamEvent {
            event_type: "patchset-created".to_string(),
            project: Some("vendor/telephony".to_string()),
            branch: Some("refs/heads/akashic/auto-fix".to_string()),
            topic: None,
            change_id: Some("I222".to_string()),
            change_number: Some(2),
            revision: None,
            commit_message: Some("regular fix".to_string()),
            comment: None,
            event_created_on: None,
            raw_json: serde_json::Value::Null,
        };
        assert!(ev_branch.is_self_triggered());

        let ev_commit = GerritStreamEvent {
            event_type: "change-merged".to_string(),
            project: Some("vendor/telephony".to_string()),
            branch: Some("master".to_string()),
            topic: None,
            change_id: Some("I333".to_string()),
            change_number: Some(3),
            revision: None,
            commit_message: Some("[akashic-crystallize] auto-generated incident doc".to_string()),
            comment: None,
            event_created_on: None,
            raw_json: serde_json::Value::Null,
        };
        assert!(ev_commit.is_self_triggered());

        let ev_comment = GerritStreamEvent {
            event_type: "comment-added".to_string(),
            project: Some("vendor/telephony".to_string()),
            branch: Some("master".to_string()),
            topic: None,
            change_id: Some("I444".to_string()),
            change_number: Some(4),
            revision: None,
            commit_message: Some("normal commit".to_string()),
            comment: Some("See [akashic-crystallize] for details".to_string()),
            event_created_on: None,
            raw_json: serde_json::Value::Null,
        };
        assert!(ev_comment.is_self_triggered());

        let ev_custom = GerritStreamEvent {
            event_type: "patchset-created".to_string(),
            project: Some("custom-prefix/app".to_string()),
            branch: Some("master".to_string()),
            topic: None,
            change_id: Some("I555".to_string()),
            change_number: Some(5),
            revision: None,
            commit_message: Some("custom update".to_string()),
            comment: None,
            event_created_on: None,
            raw_json: serde_json::Value::Null,
        };
        assert!(is_self_triggered_event(&ev_custom, Some("custom-prefix/")));
        assert!(!is_self_triggered_event(&ev_custom, None));
    }
}
