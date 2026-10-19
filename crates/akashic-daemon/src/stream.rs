use std::time::Duration;
use crate::DaemonConfig;

#[derive(Debug, Clone)]
pub struct BackoffPolicy {
    pub base_delay_ms: u64,
    pub max_delay_ms: u64,
}

impl Default for BackoffPolicy {
    fn default() -> Self {
        Self {
            base_delay_ms: 1_000,
            max_delay_ms: 60_000,
        }
    }
}

impl BackoffPolicy {
    pub fn new(base_delay_ms: u64, max_delay_ms: u64) -> Self {
        Self {
            base_delay_ms,
            max_delay_ms,
        }
    }

    pub fn base_delay_for_attempt(&self, attempt: u32) -> Duration {
        let shift = attempt.min(10);
        let factor = 1u64.checked_shl(shift).unwrap_or(u64::MAX);
        let raw_delay = self.base_delay_ms.saturating_mul(factor);
        let capped = raw_delay.min(self.max_delay_ms);
        Duration::from_millis(capped)
    }

    pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
        let base = self.base_delay_for_attempt(attempt);
        let jitter_limit = self.base_delay_ms / 2;
        let jitter = if jitter_limit > 0 {
            fastrand::u64(0..=jitter_limit)
        } else {
            0
        };
        base + Duration::from_millis(jitter)
    }
}

pub fn build_ssh_stream_args(config: &DaemonConfig) -> Vec<String> {
    vec![
        "-p".to_string(),
        config.gerrit_port.to_string(),
        config.gerrit_host.clone(),
        "gerrit".to_string(),
        "stream-events".to_string(),
        "-s".to_string(),
        "patchset-created".to_string(),
        "-s".to_string(),
        "change-merged".to_string(),
        "-s".to_string(),
        "change-abandoned".to_string(),
        "-s".to_string(),
        "comment-added".to_string(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backoff_policy_growth_and_cap() {
        let policy = BackoffPolicy::default();

        assert_eq!(policy.base_delay_for_attempt(0), Duration::from_millis(1000));
        assert_eq!(policy.base_delay_for_attempt(1), Duration::from_millis(2000));
        assert_eq!(policy.base_delay_for_attempt(2), Duration::from_millis(4000));
        assert_eq!(policy.base_delay_for_attempt(3), Duration::from_millis(8000));
        assert_eq!(policy.base_delay_for_attempt(4), Duration::from_millis(16000));
        assert_eq!(policy.base_delay_for_attempt(5), Duration::from_millis(32000));
        assert_eq!(policy.base_delay_for_attempt(6), Duration::from_millis(60000)); // capped
        assert_eq!(policy.base_delay_for_attempt(10), Duration::from_millis(60000));
    }

    #[test]
    fn test_build_ssh_stream_args() {
        let config = DaemonConfig::default();
        let args = build_ssh_stream_args(&config);

        assert!(args.contains(&"-p".to_string()));
        assert!(args.contains(&"29418".to_string()));
        assert!(args.contains(&"gerrit.mot.com".to_string()));
        assert!(args.contains(&"stream-events".to_string()));
    }

    #[test]
    fn test_backoff_jitter_bounds() {
        let policy = BackoffPolicy::default();
        for _ in 0..100 {
            let delay = policy.delay_for_attempt(1);
            assert!(delay >= Duration::from_millis(2000));
            assert!(delay <= Duration::from_millis(2500));
        }
    }
}
