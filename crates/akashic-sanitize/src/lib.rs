use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SanitizeError {
    #[error("Sanitization processing failed: {0}")]
    ProcessingFailed(String),
    #[error("Invalid input format: {0}")]
    InvalidInput(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SanitizedOutput {
    pub sanitized_content: String,
    pub redactions_count: usize,
}

pub trait SanitizerProfile: Send + Sync {
    fn sanitize(&self, input: &str) -> SanitizedOutput;
}

#[derive(Debug, Clone)]
pub struct CodeProfile {
    secret_patterns: Vec<Regex>,
}

impl CodeProfile {
    pub fn new() -> Self {
        Self {
            secret_patterns: vec![
                // Prefix-based tokens (e.g. GitHub PAT, OpenAI key, generic secret)
                Regex::new(r"\b(?:ghp_[A-Za-z0-9]{36}|sk-[A-Za-z0-9]{20,}|secret_[A-Za-z0-9]{16,})\b")
                    .expect("valid token regex"),
                // Key-value assignments
                Regex::new(r#"(?i)\b((?:api[_-]?key|auth[_-]?token|access[_-]?token|secret[_-]?key|bearer)\s*[:=]\s*["']?)([A-Za-z0-9_\-\.]{16,})(["']?)"#)
                    .expect("valid assignment regex"),
            ],
        }
    }
}

impl Default for CodeProfile {
    fn default() -> Self {
        Self::new()
    }
}

impl SanitizerProfile for CodeProfile {
    fn sanitize(&self, input: &str) -> SanitizedOutput {
        let mut count = 0;
        let mut result = input.to_string();

        // 1. Direct tokens
        result = self.secret_patterns[0].replace_all(&result, |_caps: &regex::Captures| {
            count += 1;
            "SAN_STR_TOKEN_0001".to_string()
        }).into_owned();

        // 2. Assignment tokens
        result = self.secret_patterns[1].replace_all(&result, |caps: &regex::Captures| {
            count += 1;
            format!("{}SAN_STR_TOKEN_0001{}", &caps[1], &caps[3])
        }).into_owned();

        SanitizedOutput {
            sanitized_content: result,
            redactions_count: count,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LogProfile {
    salt: String,
    ipv4_regex: Regex,
    mac_regex: Regex,
    phone_regex: Regex,
}

impl LogProfile {
    pub fn new() -> Self {
        Self::with_salt("akashic-default-salt")
    }

    pub fn with_salt(salt: impl Into<String>) -> Self {
        Self {
            salt: salt.into(),
            ipv4_regex: Regex::new(r"\b(?:(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\.){3}(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\b")
                .expect("valid ipv4 regex"),
            mac_regex: Regex::new(r"\b(?:[0-9A-Fa-f]{2}[:-]){5}[0-9A-Fa-f]{2}\b")
                .expect("valid mac regex"),
            phone_regex: Regex::new(r"(?:\+1[- ]?)?\(?\d{3}\)?[- ]?\d{3}[- ]?\d{4}\b")
                .expect("valid phone regex"),
        }
    }

    fn pseudonymize_ipv4(&self, ip: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.salt.as_bytes());
        hasher.update(ip.as_bytes());
        let hash = hasher.finalize();
        format!("10.{}.{}.{}", hash[0], hash[1], hash[2])
    }

    fn pseudonymize_mac(&self, mac: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.salt.as_bytes());
        hasher.update(mac.as_bytes());
        let hash = hasher.finalize();
        format!(
            "02:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            hash[0], hash[1], hash[2], hash[3], hash[4]
        )
    }

    fn pseudonymize_phone(&self, phone: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.salt.as_bytes());
        hasher.update(phone.as_bytes());
        let hash = hasher.finalize();
        let num1 = 555;
        let num2 = (hash[0] as u16 * 256 + hash[1] as u16) % 9000 + 1000;
        format!("555-{num1:03}-{num2:04}")
    }
}

impl Default for LogProfile {
    fn default() -> Self {
        Self::new()
    }
}

impl SanitizerProfile for LogProfile {
    fn sanitize(&self, input: &str) -> SanitizedOutput {
        let mut count = 0;
        let mut result = input.to_string();

        result = self.ipv4_regex.replace_all(&result, |caps: &regex::Captures| {
            count += 1;
            self.pseudonymize_ipv4(&caps[0])
        }).into_owned();

        result = self.mac_regex.replace_all(&result, |caps: &regex::Captures| {
            count += 1;
            self.pseudonymize_mac(&caps[0])
        }).into_owned();

        result = self.phone_regex.replace_all(&result, |caps: &regex::Captures| {
            count += 1;
            self.pseudonymize_phone(&caps[0])
        }).into_owned();

        SanitizedOutput {
            sanitized_content: result,
            redactions_count: count,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_code_profile_sanitization() {
        let profile = CodeProfile::new();
        let input = r#"let secret = "ghp_123456789012345678901234567890123456";
api_key = "abcdef1234567890abcdef1234567890";"#;
        let output = profile.sanitize(input);
        assert_eq!(output.redactions_count, 2);
        assert!(output.sanitized_content.contains("SAN_STR_TOKEN_0001"));
        assert!(!output.sanitized_content.contains("ghp_1234567890"));
    }

    #[test]
    fn test_log_profile_sanitization() {
        let profile = LogProfile::new();
        let input = "Client 192.168.1.50 with MAC 00:1A:2B:3C:4D:5E called 555-123-4567";
        let output = profile.sanitize(input);
        assert_eq!(output.redactions_count, 3);
        assert!(!output.sanitized_content.contains("192.168.1.50"));
        assert!(!output.sanitized_content.contains("00:1A:2B:3C:4D:5E"));
        assert!(!output.sanitized_content.contains("555-123-4567"));
    }
}
