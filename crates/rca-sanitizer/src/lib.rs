use regex::{Captures, Regex};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SanitizerError {
    #[error("Sanitization processing failed: {0}")]
    ProcessingFailed(String),
    #[error("Invalid input format: {0}")]
    InvalidInput(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SanitizedOutput {
    pub sanitized_content: String,
    pub redactions_count: usize,
}

pub trait Sanitizer {
    type Error: std::error::Error + Send + Sync + 'static;

    fn sanitize(&self, raw: &str) -> Result<SanitizedOutput, Self::Error>;
}

#[derive(Debug, Default, Clone)]
pub struct BasicSanitizer;

impl Sanitizer for BasicSanitizer {
    type Error = SanitizerError;

    fn sanitize(&self, raw: &str) -> Result<SanitizedOutput, Self::Error> {
        Ok(SanitizedOutput {
            sanitized_content: raw.to_string(),
            redactions_count: 0,
        })
    }
}

#[derive(Debug, Clone)]
pub struct FormatPreservingSanitizer {
    pem_regex: Regex,
    oem_assignment_regex: Regex,
    key_assignment_regex: Regex,
    bearer_regex: Regex,
    jwt_regex: Regex,
    cred_prefix_regex: Regex,
    email_regex: Regex,
    imei_regex: Regex,
    phone_regex: Regex,
}

impl Default for FormatPreservingSanitizer {
    fn default() -> Self {
        Self::new()
    }
}

impl FormatPreservingSanitizer {
    pub fn new() -> Self {
        Self {
            pem_regex: Regex::new(
                r"(?s)-----BEGIN [A-Z0-9 _-]+-----[^\r\n]*\r?\n.+?-----END [A-Z0-9 _-]+-----",
            )
            .expect("Invalid PEM regex"),
            oem_assignment_regex: Regex::new(
                r#"(?i)\b((?:oem[_-]?(?:private[_-]?)?key|signing[_-]?key|oem[_-]?secret)\s*[:=]\s*["']?)([^\s"'\r\n]+)(["']?)"#,
            )
            .expect("Invalid OEM assignment regex"),
            key_assignment_regex: Regex::new(
                r#"(?i)\b((?:api[_-]?key|auth[_-]?token|access[_-]?token|secret[_-]?key)\s*[:=]\s*["']?)([A-Za-z0-9_\-\.]{16,})(["']?)"#,
            )
            .expect("Invalid key assignment regex"),
            bearer_regex: Regex::new(r"(?i)\b(Bearer\s+)([A-Za-z0-9_\-\.]{16,})")
                .expect("Invalid Bearer regex"),
            jwt_regex: Regex::new(
                r"\beyJ[A-Za-z0-9_-]{10,}\.eyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\b",
            )
            .expect("Invalid JWT regex"),
            cred_prefix_regex: Regex::new(
                r"\b(?:ghp_[A-Za-z0-9]{36}|sk-[A-Za-z0-9]{20,}|secret_[A-Za-z0-9]{16,})\b",
            )
            .expect("Invalid cred prefix regex"),
            email_regex: Regex::new(r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}\b")
                .expect("Invalid email regex"),
            imei_regex: Regex::new(r"\b(?:\d{15}|\d{2}-\d{6}-\d{6}-\d)\b")
                .expect("Invalid IMEI regex"),
            phone_regex: Regex::new(
                r"(?:\+1[- ]?\(?\d{3}\)?[- ]?\d{3}[- ]?\d{4}\b|\b1-\d{3}-\d{3}-\d{4}\b|\(\d{3}\)\s*\d{3}-\d{4}\b|\b\d{3}-\d{3}-\d{4}\b|\b\d{10}\b)",
            )
            .expect("Invalid phone regex"),
        }
    }
}

impl Sanitizer for FormatPreservingSanitizer {
    type Error = SanitizerError;

    fn sanitize(&self, raw: &str) -> Result<SanitizedOutput, Self::Error> {
        let mut total_redactions = 0;

        // 1. OEM key assignments -> <OEM_SECRET:REDACTED>
        let content = self
            .oem_assignment_regex
            .replace_all(raw, |caps: &Captures| {
                let val = &caps[2];
                if val.contains("REDACTED") {
                    caps[0].to_string()
                } else {
                    total_redactions += 1;
                    format!("{}<OEM_SECRET:REDACTED>{}", &caps[1], &caps[3])
                }
            })
            .into_owned();

        // 2. PEM blocks -> <OEM_SECRET:REDACTED>
        let content = self
            .pem_regex
            .replace_all(&content, |_caps: &Captures| {
                total_redactions += 1;
                "<OEM_SECRET:REDACTED>".to_string()
            })
            .into_owned();

        // 3. Key assignments -> <TOKEN:REDACTED>
        let content = self
            .key_assignment_regex
            .replace_all(&content, |caps: &Captures| {
                let val = &caps[2];
                if val == "<TOKEN:REDACTED>" || val == "<OEM_SECRET:REDACTED>" {
                    caps[0].to_string()
                } else {
                    total_redactions += 1;
                    format!("{}<TOKEN:REDACTED>{}", &caps[1], &caps[3])
                }
            })
            .into_owned();

        // 4. Bearer tokens -> Bearer <TOKEN:REDACTED>
        let content = self
            .bearer_regex
            .replace_all(&content, |caps: &Captures| {
                let val = &caps[2];
                if val == "<TOKEN:REDACTED>" {
                    caps[0].to_string()
                } else {
                    total_redactions += 1;
                    format!("{}<TOKEN:REDACTED>", &caps[1])
                }
            })
            .into_owned();

        // 5. Credential prefixes -> <TOKEN:REDACTED>
        let content = self
            .cred_prefix_regex
            .replace_all(&content, |_caps: &Captures| {
                total_redactions += 1;
                "<TOKEN:REDACTED>".to_string()
            })
            .into_owned();

        // 6. JWT tokens -> <TOKEN:REDACTED>
        let content = self
            .jwt_regex
            .replace_all(&content, |_caps: &Captures| {
                total_redactions += 1;
                "<TOKEN:REDACTED>".to_string()
            })
            .into_owned();

        // 7. Email addresses -> <EMAIL:REDACTED>
        let content = self
            .email_regex
            .replace_all(&content, |_caps: &Captures| {
                total_redactions += 1;
                "<EMAIL:REDACTED>".to_string()
            })
            .into_owned();

        // 8. Device IMEIs -> <IMEI:VALID_15DIGIT>
        let content = self
            .imei_regex
            .replace_all(&content, |_caps: &Captures| {
                total_redactions += 1;
                "<IMEI:VALID_15DIGIT>".to_string()
            })
            .into_owned();

        // 9. Phone numbers -> <PHONE:US_VALID_10DIGIT>
        let content = self
            .phone_regex
            .replace_all(&content, |_caps: &Captures| {
                total_redactions += 1;
                "<PHONE:US_VALID_10DIGIT>".to_string()
            })
            .into_owned();

        Ok(SanitizedOutput {
            sanitized_content: content,
            redactions_count: total_redactions,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_phone_number_redaction() {
        let sanitizer = FormatPreservingSanitizer::new();

        let phones = [
            ("+1-555-123-4567", "<PHONE:US_VALID_10DIGIT>"),
            ("(555) 123-4567", "<PHONE:US_VALID_10DIGIT>"),
            ("555-123-4567", "<PHONE:US_VALID_10DIGIT>"),
            ("1-555-123-4567", "<PHONE:US_VALID_10DIGIT>"),
            ("5551234567", "<PHONE:US_VALID_10DIGIT>"),
        ];

        for (input, expected) in phones {
            let res = sanitizer.sanitize(input).unwrap();
            assert_eq!(res.sanitized_content, expected, "Failed for {}", input);
            assert_eq!(res.redactions_count, 1);
        }
    }

    #[test]
    fn test_imei_redaction() {
        let sanitizer = FormatPreservingSanitizer::new();

        let imeis = [
            ("352099001761482", "<IMEI:VALID_15DIGIT>"),
            ("35-209900-176148-2", "<IMEI:VALID_15DIGIT>"),
        ];

        for (input, expected) in imeis {
            let res = sanitizer.sanitize(input).unwrap();
            assert_eq!(res.sanitized_content, expected);
            assert_eq!(res.redactions_count, 1);
        }
    }

    #[test]
    fn test_email_redaction() {
        let sanitizer = FormatPreservingSanitizer::new();
        let input = "Contact engineer user.name+tag@motorola.com for details";
        let res = sanitizer.sanitize(input).unwrap();
        assert_eq!(
            res.sanitized_content,
            "Contact engineer <EMAIL:REDACTED> for details"
        );
        assert_eq!(res.redactions_count, 1);
    }

    #[test]
    fn test_token_and_secret_redactions() {
        let sanitizer = FormatPreservingSanitizer::new();

        // Bearer
        let bearer_input = "Authorization: Bearer abcd1234efgh5678ijkl9012";
        let res = sanitizer.sanitize(bearer_input).unwrap();
        assert_eq!(
            res.sanitized_content,
            "Authorization: Bearer <TOKEN:REDACTED>"
        );
        assert_eq!(res.redactions_count, 1);

        // Key assignments
        let key_input = "api_key: \"abcdef1234567890xyz\"";
        let res = sanitizer.sanitize(key_input).unwrap();
        assert_eq!(res.sanitized_content, "api_key: \"<TOKEN:REDACTED>\"");
        assert_eq!(res.redactions_count, 1);

        let auth_input = "auth_token=supersecrettoken123456";
        let res = sanitizer.sanitize(auth_input).unwrap();
        assert_eq!(res.sanitized_content, "auth_token=<TOKEN:REDACTED>");
        assert_eq!(res.redactions_count, 1);

        // Standard credential prefixes
        let ghp_input = "Token: ghp_123456789012345678901234567890123456";
        let res = sanitizer.sanitize(ghp_input).unwrap();
        assert_eq!(res.sanitized_content, "Token: <TOKEN:REDACTED>");
        assert_eq!(res.redactions_count, 1);

        let sk_input = "Key: sk-12345678901234567890abcdef";
        let res = sanitizer.sanitize(sk_input).unwrap();
        assert_eq!(res.sanitized_content, "Key: <TOKEN:REDACTED>");
        assert_eq!(res.redactions_count, 1);

        // JWT
        let jwt_input = "jwt: eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHpunv_w";
        let res = sanitizer.sanitize(jwt_input).unwrap();
        assert_eq!(res.sanitized_content, "jwt: <TOKEN:REDACTED>");
        assert_eq!(res.redactions_count, 1);
    }

    #[test]
    fn test_oem_secret_redactions() {
        let sanitizer = FormatPreservingSanitizer::new();

        let pem_input = "-----BEGIN PRIVATE KEY-----\nMIIEvgIBADANBgkqhkiG9w0BAQEFAASCBKgwggSkAgEAAoIBAQC3\n-----END PRIVATE KEY-----";
        let res = sanitizer.sanitize(pem_input).unwrap();
        assert_eq!(res.sanitized_content, "<OEM_SECRET:REDACTED>");
        assert_eq!(res.redactions_count, 1);

        let oem_key_input = "signing_key: \"secret_oem_signing_blob_xyz\"";
        let res = sanitizer.sanitize(oem_key_input).unwrap();
        assert_eq!(
            res.sanitized_content,
            "signing_key: \"<OEM_SECRET:REDACTED>\""
        );
        assert_eq!(res.redactions_count, 1);
    }

    #[test]
    fn test_combined_diagnostics_preservation() {
        let sanitizer = FormatPreservingSanitizer::new();

        let log = r#"
2026-10-19 12:00:00 [DEBUG] Device IMEI: 352099001761482
2026-10-19 12:00:01 [INFO] Caller: +1-555-123-4567 email: dev@example.com
2026-10-19 12:00:02 [ERROR] Crash at com.motorola.camera3.CameraDeviceClient.notifyError(CameraDeviceClient.java:412)
Auth header: Bearer abcdef1234567890abcdef
"#;
        let res = sanitizer.sanitize(log).unwrap();
        assert!(res.sanitized_content.contains("Device IMEI: <IMEI:VALID_15DIGIT>"));
        assert!(res.sanitized_content.contains("Caller: <PHONE:US_VALID_10DIGIT> email: <EMAIL:REDACTED>"));
        assert!(res.sanitized_content.contains("at com.motorola.camera3.CameraDeviceClient.notifyError(CameraDeviceClient.java:412)"));
        assert!(res.sanitized_content.contains("Bearer <TOKEN:REDACTED>"));
        assert_eq!(res.redactions_count, 4);
    }
}
