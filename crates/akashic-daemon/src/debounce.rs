use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::events::{should_process_event, GerritStreamEvent, SELF_TRIGGER_PREFIX};

#[derive(Debug, Clone)]
pub struct CoalescedProjectBatch {
    pub project: String,
    pub first_event_at: Instant,
    pub last_event_at: Instant,
    pub events: Vec<GerritStreamEvent>,
    pub latest_revision: Option<String>,
    pub latest_change_id: Option<String>,
    pub latest_timestamp: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct PendingProjectBatch {
    pub project: String,
    pub first_event_at: Instant,
    pub last_event_at: Instant,
    pub quiet_deadline: Instant,
    pub max_cap_deadline: Instant,
    pub events: Vec<GerritStreamEvent>,
    pub latest_revision: Option<String>,
    pub latest_change_id: Option<String>,
    pub latest_timestamp: Option<i64>,
}

impl PendingProjectBatch {
    pub fn effective_deadline(&self) -> Instant {
        self.quiet_deadline.min(self.max_cap_deadline)
    }
}

pub struct DebounceQueue {
    quiet_window: Duration,
    max_wait_cap: Duration,
    loop_drop_prefix: String,
    pending: HashMap<String, PendingProjectBatch>,
}

impl DebounceQueue {
    pub fn new(quiet_window: Duration, max_wait_cap: Duration) -> Self {
        Self {
            quiet_window,
            max_wait_cap,
            loop_drop_prefix: SELF_TRIGGER_PREFIX.to_string(),
            pending: HashMap::new(),
        }
    }

    pub fn from_secs(quiet_secs: u64, max_wait_secs: u64) -> Self {
        Self::new(
            Duration::from_secs(quiet_secs),
            Duration::from_secs(max_wait_secs),
        )
    }

    pub fn with_loop_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.loop_drop_prefix = prefix.into();
        self
    }

    pub fn push(&mut self, event: GerritStreamEvent) -> bool {
        self.push_at(event, Instant::now())
    }

    pub fn push_at(&mut self, event: GerritStreamEvent, now: Instant) -> bool {
        if !should_process_event(&event, Some(&self.loop_drop_prefix)) {
            return false;
        }

        let project = match &event.project {
            Some(p) if !p.trim().is_empty() => p.clone(),
            _ => return false,
        };

        let revision = event.revision.clone();
        let change_id = event.change_id.clone();
        let timestamp = event.event_created_on;

        if let Some(batch) = self.pending.get_mut(&project) {
            batch.last_event_at = now;
            let proposed_quiet = now + self.quiet_window;
            batch.quiet_deadline = proposed_quiet.min(batch.max_cap_deadline);

            if revision.is_some() {
                batch.latest_revision = revision;
            }
            if change_id.is_some() {
                batch.latest_change_id = change_id;
            }
            if let Some(ts) = timestamp {
                batch.latest_timestamp = Some(
                    batch
                        .latest_timestamp
                        .map_or(ts, |existing| existing.max(ts)),
                );
            }
            batch.events.push(event);
        } else {
            let max_cap_deadline = now + self.max_wait_cap;
            let quiet_deadline = (now + self.quiet_window).min(max_cap_deadline);

            self.pending.insert(
                project.clone(),
                PendingProjectBatch {
                    project,
                    first_event_at: now,
                    last_event_at: now,
                    quiet_deadline,
                    max_cap_deadline,
                    events: vec![event],
                    latest_revision: revision,
                    latest_change_id: change_id,
                    latest_timestamp: timestamp,
                },
            );
        }

        true
    }

    pub fn pop_ready(&mut self, now: Instant) -> Vec<CoalescedProjectBatch> {
        let mut ready_projects = Vec::new();
        for (project, batch) in &self.pending {
            if now >= batch.effective_deadline() {
                ready_projects.push(project.clone());
            }
        }

        ready_projects.sort();

        let mut ready_batches = Vec::with_capacity(ready_projects.len());
        for project in ready_projects {
            if let Some(batch) = self.pending.remove(&project) {
                ready_batches.push(CoalescedProjectBatch {
                    project: batch.project,
                    first_event_at: batch.first_event_at,
                    last_event_at: batch.last_event_at,
                    events: batch.events,
                    latest_revision: batch.latest_revision,
                    latest_change_id: batch.latest_change_id,
                    latest_timestamp: batch.latest_timestamp,
                });
            }
        }

        ready_batches
    }

    pub fn next_deadline(&self) -> Option<Instant> {
        self.pending
            .values()
            .map(|b| b.effective_deadline())
            .min()
    }

    pub fn pending_projects(&self) -> usize {
        self.pending.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    pub fn get_pending(&self, project: &str) -> Option<&PendingProjectBatch> {
        self.pending.get(project)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_event(project: &str, change_num: i64, rev: &str) -> GerritStreamEvent {
        GerritStreamEvent {
            event_type: "change-merged".to_string(),
            project: Some(project.to_string()),
            branch: Some("master".to_string()),
            topic: None,
            change_id: Some(format!("I{}", change_num)),
            change_number: Some(change_num),
            revision: Some(rev.to_string()),
            commit_message: Some(format!("Commit {}", change_num)),
            comment: None,
            event_created_on: Some(1700000000 + change_num),
            raw_json: serde_json::Value::Null,
        }
    }

    #[test]
    fn test_debounce_quiet_sliding_and_batching() {
        let start = Instant::now();
        let mut queue = DebounceQueue::from_secs(30, 300);

        // Push event 1 at t=0
        assert!(queue.push_at(dummy_event("proj-a", 1, "rev1"), start));
        assert_eq!(queue.pending_projects(), 1);

        // Not ready at t=15
        let t15 = start + Duration::from_secs(15);
        assert!(queue.pop_ready(t15).is_empty());

        // Push event 2 at t=20
        let t20 = start + Duration::from_secs(20);
        assert!(queue.push_at(dummy_event("proj-a", 2, "rev2"), t20));

        // At t=35 (original quiet window was 30s), it slid by 30s from t=20 -> t=50s
        let t35 = start + Duration::from_secs(35);
        assert!(queue.pop_ready(t35).is_empty());

        // At t=50, the quiet window expires!
        let t50 = start + Duration::from_secs(50);
        let batches = queue.pop_ready(t50);
        assert_eq!(batches.len(), 1);
        let batch = &batches[0];
        assert_eq!(batch.project, "proj-a");
        assert_eq!(batch.events.len(), 2);
        assert_eq!(batch.latest_revision.as_deref(), Some("rev2"));
        assert!(queue.is_empty());
    }

    #[test]
    fn test_debounce_max_wait_cap_enforcement() {
        let start = Instant::now();
        let mut queue = DebounceQueue::from_secs(30, 300);

        // Push continuous events every 20s
        for sec in (0..320).step_by(20) {
            let t = start + Duration::from_secs(sec);
            queue.push_at(dummy_event("proj-cap", sec as i64, &format!("rev{}", sec)), t);
        }

        // At t=280, max cap is 300s, quiet deadline would want to be 280+30=310, but capped at 300
        let t280 = start + Duration::from_secs(280);
        let pending = queue.get_pending("proj-cap").unwrap();
        assert_eq!(pending.effective_deadline(), start + Duration::from_secs(300));
        assert!(queue.pop_ready(t280).is_empty());

        // At t=300, max cap expires and batch is popped!
        let t300 = start + Duration::from_secs(300);
        let batches = queue.pop_ready(t300);
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].project, "proj-cap");
    }

    #[test]
    fn test_debounce_drops_self_triggered_and_empty_project() {
        let mut queue = DebounceQueue::from_secs(30, 300);

        let self_triggered = GerritStreamEvent {
            event_type: "change-merged".to_string(),
            project: Some("akashic/repo".to_string()),
            branch: Some("master".to_string()),
            topic: None,
            change_id: Some("I111".to_string()),
            change_number: Some(1),
            revision: None,
            commit_message: None,
            comment: None,
            event_created_on: None,
            raw_json: serde_json::Value::Null,
        };
        assert!(!queue.push(self_triggered));

        let empty_proj = GerritStreamEvent {
            event_type: "change-merged".to_string(),
            project: Some("   ".to_string()),
            branch: Some("master".to_string()),
            topic: None,
            change_id: Some("I222".to_string()),
            change_number: Some(2),
            revision: None,
            commit_message: None,
            comment: None,
            event_created_on: None,
            raw_json: serde_json::Value::Null,
        };
        assert!(!queue.push(empty_proj));
        assert!(queue.is_empty());
    }

    #[test]
    fn test_multi_project_debounce() {
        let start = Instant::now();
        let mut queue = DebounceQueue::from_secs(30, 300);

        queue.push_at(dummy_event("proj-1", 1, "r1"), start);
        queue.push_at(dummy_event("proj-2", 2, "r2"), start + Duration::from_secs(10));

        assert_eq!(queue.pending_projects(), 2);
        assert_eq!(queue.next_deadline(), Some(start + Duration::from_secs(30)));

        // At t=30, proj-1 is ready, proj-2 is not ready (deadline is t=40)
        let popped30 = queue.pop_ready(start + Duration::from_secs(30));
        assert_eq!(popped30.len(), 1);
        assert_eq!(popped30[0].project, "proj-1");
        assert_eq!(queue.pending_projects(), 1);

        // At t=40, proj-2 is ready
        let popped40 = queue.pop_ready(start + Duration::from_secs(40));
        assert_eq!(popped40.len(), 1);
        assert_eq!(popped40[0].project, "proj-2");
        assert!(queue.is_empty());
    }
}
