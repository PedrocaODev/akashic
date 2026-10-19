use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SymbolicatorError {
    #[error("Failed to parse stack frame: {0}")]
    ParseError(String),
    #[error("Symbol resolution failed: {0}")]
    ResolutionFailed(String),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StackFrame {
    pub instruction_addr: u64,
    pub module_name: Option<String>,
    pub raw_symbol: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SymbolicatedFrame {
    pub frame_index: usize,
    pub file: Option<String>,
    pub line: Option<u32>,
    pub symbol: String,
    pub class_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FaultingSymbol {
    pub file: String,
    pub line: u32,
    pub symbol: String,
    pub class_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SymbolicationOutput {
    pub crash_signature: String,
    pub is_native_crash: bool,
    pub symbolicated_stack_trace: String,
    pub faulting_symbol: Option<FaultingSymbol>,
    pub symbol_locations: Vec<SymbolicatedFrame>,
    pub evidence_payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResolvedSymbol {
    pub repo_name: String,
    pub package_id: String,
    pub file: Option<String>,
    pub line: Option<u32>,
    pub symbol: String,
    pub class_name: Option<String>,
}

/// Computes the deterministic SHA-256 crash signature:
/// sha256(format!("{}:{}:{}", app_id, exception_or_signal, top_frames.join(",")))
pub fn compute_crash_signature(
    app_id: &str,
    exception_or_signal: &str,
    top_frames: &[String],
) -> String {
    let mut hasher = Sha256::new();
    let data = format!("{}:{}:{}", app_id, exception_or_signal, top_frames.join(","));
    hasher.update(data.as_bytes());
    format!("{:064x}", hasher.finalize())
}

pub trait SymbolResolver: Send + Sync {
    fn resolve_symbol(
        &self,
        raw_symbol: &str,
        crash_app_id: Option<&str>,
    ) -> Result<Option<ResolvedSymbol>, SymbolicatorError>;
}

/// Symbol resolver backed by Alexandria Code Graph Store
#[derive(Clone)]
pub struct AlexandriaSymbolResolver {
    store: Arc<std::sync::Mutex<alexandria_core::store::CodeGraphStore>>,
}

impl AlexandriaSymbolResolver {
    pub fn new(store: Arc<std::sync::Mutex<alexandria_core::store::CodeGraphStore>>) -> Self {
        Self { store }
    }

    pub fn from_store(store: alexandria_core::store::CodeGraphStore) -> Self {
        Self::new(Arc::new(std::sync::Mutex::new(store)))
    }
}

impl SymbolResolver for AlexandriaSymbolResolver {
    fn resolve_symbol(
        &self,
        raw_symbol: &str,
        crash_app_id: Option<&str>,
    ) -> Result<Option<ResolvedSymbol>, SymbolicatorError> {
        let store = self
            .store
            .lock()
            .map_err(|e| SymbolicatorError::ResolutionFailed(format!("Lock poisoned: {}", e)))?;
        let res = store
            .resolve_stack_symbol(raw_symbol, crash_app_id)
            .map_err(|e| SymbolicatorError::ResolutionFailed(e.to_string()))?;

        if let Some(stack_sym) = res {
            Ok(Some(ResolvedSymbol {
                repo_name: stack_sym.repo_name,
                package_id: stack_sym.package_id,
                file: None,
                line: None,
                symbol: raw_symbol.to_string(),
                class_name: None,
            }))
        } else {
            Ok(None)
        }
    }
}

/// In-memory symbol resolver for testing and offline execution
#[derive(Debug, Default, Clone)]
pub struct InMemorySymbolResolver {
    symbols_by_app: HashMap<(String, String), ResolvedSymbol>,
    symbols_generic: HashMap<String, ResolvedSymbol>,
}

impl InMemorySymbolResolver {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_symbol(
        &mut self,
        raw_symbol: impl Into<String>,
        crash_app_id: Option<impl Into<String>>,
        resolved: ResolvedSymbol,
    ) {
        let sym = raw_symbol.into();
        if let Some(app) = crash_app_id {
            self.symbols_by_app
                .insert((sym.clone(), app.into()), resolved.clone());
        }
        self.symbols_generic.insert(sym, resolved);
    }
}

impl SymbolResolver for InMemorySymbolResolver {
    fn resolve_symbol(
        &self,
        raw_symbol: &str,
        crash_app_id: Option<&str>,
    ) -> Result<Option<ResolvedSymbol>, SymbolicatorError> {
        if let Some(app) = crash_app_id {
            if let Some(res) = self
                .symbols_by_app
                .get(&(raw_symbol.to_string(), app.to_string()))
            {
                return Ok(Some(res.clone()));
            }
        }
        if let Some(res) = self.symbols_generic.get(raw_symbol) {
            return Ok(Some(res.clone()));
        }
        Ok(None)
    }
}

pub struct Symbolicator<R: SymbolResolver> {
    resolver: R,
    java_frame_regex: Regex,
    native_frame_regex: Regex,
    signal_regex: Regex,
    java_exception_regex: Regex,
}

impl<R: SymbolResolver> Symbolicator<R> {
    pub fn new(resolver: R) -> Self {
        Self {
            resolver,
            java_frame_regex: Regex::new(
                r#"^\s*at\s+([a-zA-Z0-9_$]+(?:\.[a-zA-Z0-9_$]+)+)\.([a-zA-Z0-9_$<>]+)\((?:([a-zA-Z0-9_$-]+\.(?:java|kt|scala)):(\d+)|([^)]+))\)"#,
            )
            .expect("Valid Java regex"),
            native_frame_regex: Regex::new(
                r#"^\s*#(\d+)\s+pc\s+([0-9a-fA-F]+)\s+(\S+)(?:\s+\((.+?)(?:\+(\d+))?\))?(?:\s+\(BuildId:.*\))?"#,
            )
            .expect("Valid native regex"),
            signal_regex: Regex::new(r#"(?i)signal\s+\d+\s*\((SIG[A-Z]+)\)|(?i)\b(SIG[A-Z]+)\b"#)
                .expect("Valid signal regex"),
            java_exception_regex: Regex::new(r#"([a-zA-Z0-9_$.]+(?:Exception|Error)):?"#)
                .expect("Valid java exception regex"),
        }
    }

    pub fn symbolicate(
        &self,
        raw_stack_trace: &str,
        app_id: &str,
    ) -> Result<SymbolicationOutput, SymbolicatorError> {
        let is_native_crash = raw_stack_trace.lines().any(|l| {
            let t = l.trim_start();
            t.starts_with("#00 pc ")
                || t.starts_with("#0 pc ")
                || t.starts_with("backtrace:")
                || t.contains("BuildId:")
        });

        let exception_or_signal = if is_native_crash {
            if let Some(caps) = self.signal_regex.captures(raw_stack_trace) {
                caps.get(1)
                    .or_else(|| caps.get(2))
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_else(|| "SIGSEGV".to_string())
            } else {
                "SIGSEGV".to_string()
            }
        } else if let Some(caps) = self.java_exception_regex.captures(raw_stack_trace) {
            caps.get(1)
                .map(|m| m.as_str().to_string())
                .unwrap_or_else(|| "java.lang.RuntimeException".to_string())
        } else {
            "FATAL_EXCEPTION".to_string()
        };

        let mut symbol_locations = Vec::new();
        let mut frame_index = 0;

        if is_native_crash {
            for line in raw_stack_trace.lines() {
                if let Some(caps) = self.native_frame_regex.captures(line) {
                    let parsed_frame_idx: usize = caps
                        .get(1)
                        .and_then(|m| m.as_str().parse().ok())
                        .unwrap_or(frame_index);
                    let raw_sym = caps.get(4).map(|m| m.as_str());

                    let mut resolved_frame = if let Some(sym) = raw_sym {
                        if let Some(res) = self.resolver.resolve_symbol(sym, Some(app_id))? {
                            SymbolicatedFrame {
                                frame_index: parsed_frame_idx,
                                file: res.file,
                                line: res.line,
                                symbol: res.symbol,
                                class_name: res.class_name,
                            }
                        } else {
                            let class_name = if sym.contains("::") {
                                let parts: Vec<&str> = sym.rsplitn(2, "::").collect();
                                if parts.len() == 2 {
                                    Some(parts[1].to_string())
                                } else {
                                    None
                                }
                            } else {
                                None
                            };

                            SymbolicatedFrame {
                                frame_index: parsed_frame_idx,
                                file: None,
                                line: None,
                                symbol: sym.to_string(),
                                class_name,
                            }
                        }
                    } else {
                        let module = caps
                            .get(3)
                            .map(|m| m.as_str().to_string())
                            .unwrap_or_default();
                        SymbolicatedFrame {
                            frame_index: parsed_frame_idx,
                            file: None,
                            line: None,
                            symbol: module,
                            class_name: None,
                        }
                    };

                    resolved_frame.frame_index = parsed_frame_idx;
                    symbol_locations.push(resolved_frame);
                    frame_index += 1;
                }
            }
        } else {
            for line in raw_stack_trace.lines() {
                if let Some(caps) = self.java_frame_regex.captures(line) {
                    let class_name = caps.get(1).map(|m| m.as_str().to_string()).unwrap_or_default();
                    let method_name = caps.get(2).map(|m| m.as_str()).unwrap_or_default();
                    let full_sym = format!("{}.{}", class_name, method_name);
                    let file = caps.get(3).map(|m| m.as_str().to_string());
                    let line_num = caps.get(4).and_then(|m| m.as_str().parse::<u32>().ok());

                    let mut resolved_frame = if let Some(res) = self
                        .resolver
                        .resolve_symbol(&full_sym, Some(app_id))?
                        .or_else(|| self.resolver.resolve_symbol(&class_name, Some(app_id)).ok().flatten())
                    {
                        SymbolicatedFrame {
                            frame_index,
                            file: res.file.or(file),
                            line: res.line.or(line_num),
                            symbol: res.symbol,
                            class_name: res.class_name.or(Some(class_name)),
                        }
                    } else {
                        SymbolicatedFrame {
                            frame_index,
                            file,
                            line: line_num,
                            symbol: full_sym,
                            class_name: Some(class_name),
                        }
                    };

                    resolved_frame.frame_index = frame_index;
                    symbol_locations.push(resolved_frame);
                    frame_index += 1;
                }
            }
        }

        // Top 3 frames for crash signature
        let top_frames: Vec<String> = symbol_locations
            .iter()
            .take(3)
            .map(|f| f.symbol.clone())
            .collect();

        let crash_signature = compute_crash_signature(app_id, &exception_or_signal, &top_frames);

        // Faulting symbol is the first frame with file and line (or first frame)
        let faulting_symbol = symbol_locations
            .iter()
            .find(|f| f.file.is_some() && f.line.is_some())
            .map(|f| FaultingSymbol {
                file: f.file.clone().unwrap(),
                line: f.line.unwrap(),
                symbol: f.symbol.clone(),
                class_name: f.class_name.clone(),
            });

        let mut stack_trace_lines = Vec::new();
        if !is_native_crash {
            stack_trace_lines.push(format!("{}: fault at top frame", exception_or_signal));
            for f in &symbol_locations {
                let location = match (&f.file, f.line) {
                    (Some(file), Some(line)) => format!("{}:{}", file, line),
                    (Some(file), None) => file.clone(),
                    _ => "Unknown Source".to_string(),
                };
                stack_trace_lines.push(format!("    at {}({})", f.symbol, location));
            }
        } else {
            stack_trace_lines.push(format!("*** *** *** *** *** *** *** *** *** *** *** *** *** *** *** ***"));
            stack_trace_lines.push(format!("Fatal signal {} ({})", exception_or_signal, app_id));
            stack_trace_lines.push("backtrace:".to_string());
            for f in &symbol_locations {
                let loc = match (&f.file, f.line) {
                    (Some(file), Some(line)) => format!(" ({}:{})", file, line),
                    _ => String::new(),
                };
                stack_trace_lines.push(format!(
                    "    #{:02} pc {:08x} {}{}",
                    f.frame_index, 0, f.symbol, loc
                ));
            }
        }

        let symbolicated_stack_trace = stack_trace_lines.join("\n");

        let evidence_payload = serde_json::json!({
            "crash_signature": crash_signature,
            "is_native_crash": is_native_crash,
            "exception_or_signal": exception_or_signal,
            "app_id": app_id,
            "faulting_symbol": faulting_symbol,
            "symbol_locations": symbol_locations,
        });

        Ok(SymbolicationOutput {
            crash_signature,
            is_native_crash,
            symbolicated_stack_trace,
            faulting_symbol,
            symbol_locations,
            evidence_payload,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_crash_signature_determinism() {
        let app = "com.motorola.camera3";
        let ex = "java.lang.NullPointerException";
        let frames = vec![
            "com.motorola.camera3.CameraDeviceClient.notifyError".to_string(),
            "com.motorola.camera3.feature.Mode$1.execute".to_string(),
        ];

        let sig1 = compute_crash_signature(app, ex, &frames);
        let sig2 = compute_crash_signature(app, ex, &frames);
        assert_eq!(sig1, sig2);
        assert_eq!(sig1.len(), 64);

        let sig3 = compute_crash_signature(app, "SIGSEGV", &frames);
        assert_ne!(sig1, sig3);
    }

    #[test]
    fn test_parse_java_and_kotlin_stack_trace() {
        let trace = r#"
java.lang.NullPointerException: Attempt to read from null object
    at com.motorola.camera3.CameraDeviceClient.notifyError(CameraDeviceClient.java:412)
    at com.motorola.camera3.feature.Mode$1.execute(Mode.kt:88)
    at android.hardware.camera2.CameraDevice.notify(CameraDevice.java:102)
"#;
        let mut resolver = InMemorySymbolResolver::new();
        resolver.add_symbol(
            "com.motorola.camera3.CameraDeviceClient.notifyError",
            Some("com.motorola.camera3"),
            ResolvedSymbol {
                repo_name: "motorola-camera".to_string(),
                package_id: "com.motorola.camera3".to_string(),
                file: Some("src/com/motorola/camera3/CameraDeviceClient.java".to_string()),
                line: Some(412),
                symbol: "com.motorola.camera3.CameraDeviceClient.notifyError".to_string(),
                class_name: Some("com.motorola.camera3.CameraDeviceClient".to_string()),
            },
        );

        let symbolicator = Symbolicator::new(resolver);
        let output = symbolicator
            .symbolicate(trace, "com.motorola.camera3")
            .unwrap();

        assert!(!output.is_native_crash);
        assert_eq!(output.symbol_locations.len(), 3);

        // Frame 0 resolved with repo file path
        let f0 = &output.symbol_locations[0];
        assert_eq!(f0.frame_index, 0);
        assert_eq!(f0.symbol, "com.motorola.camera3.CameraDeviceClient.notifyError");
        assert_eq!(
            f0.file.as_deref(),
            Some("src/com/motorola/camera3/CameraDeviceClient.java")
        );
        assert_eq!(f0.line, Some(412));

        // Frame 1 Kotlin parsed
        let f1 = &output.symbol_locations[1];
        assert_eq!(f1.frame_index, 1);
        assert_eq!(f1.symbol, "com.motorola.camera3.feature.Mode$1.execute");
        assert_eq!(f1.file.as_deref(), Some("Mode.kt"));
        assert_eq!(f1.line, Some(88));

        // Faulting symbol
        let fault = output.faulting_symbol.unwrap();
        assert_eq!(
            fault.file,
            "src/com/motorola/camera3/CameraDeviceClient.java"
        );
        assert_eq!(fault.line, 412);
        assert_eq!(fault.symbol, "com.motorola.camera3.CameraDeviceClient.notifyError");
    }

    #[test]
    fn test_parse_native_tombstone_stack_trace() {
        let trace = r#"
Fatal signal 11 (SIGSEGV), code 1 (SEGV_MAPERR), fault addr 0x0 in tid 1234
backtrace:
    #00 pc 00042f10 /system/lib64/libcamera_client.so (android::CameraDeviceClient::notifyError+12)
    #01 pc 0000000000085a14 /apex/com.android.runtime/lib64/bionic/libc.so (abort+168) (BuildId: 969df6b509bc14251cecd353c713fb93)
"#;
        let mut resolver = InMemorySymbolResolver::new();
        resolver.add_symbol(
            "android::CameraDeviceClient::notifyError",
            None::<String>,
            ResolvedSymbol {
                repo_name: "frameworks-av".to_string(),
                package_id: "android.hardware.camera2".to_string(),
                file: Some("services/camera/libcameraservice/CameraDeviceClient.cpp".to_string()),
                line: Some(512),
                symbol: "android::CameraDeviceClient::notifyError".to_string(),
                class_name: Some("android::CameraDeviceClient".to_string()),
            },
        );

        let symbolicator = Symbolicator::new(resolver);
        let output = symbolicator.symbolicate(trace, "com.motorola.camera3").unwrap();

        assert!(output.is_native_crash);
        assert_eq!(output.symbol_locations.len(), 2);

        let f0 = &output.symbol_locations[0];
        assert_eq!(f0.frame_index, 0);
        assert_eq!(f0.symbol, "android::CameraDeviceClient::notifyError");
        assert_eq!(
            f0.file.as_deref(),
            Some("services/camera/libcameraservice/CameraDeviceClient.cpp")
        );
        assert_eq!(f0.line, Some(512));

        let fault = output.faulting_symbol.unwrap();
        assert_eq!(
            fault.file,
            "services/camera/libcameraservice/CameraDeviceClient.cpp"
        );
        assert_eq!(fault.line, 512);
    }

    #[test]
    fn test_alexandria_symbol_resolver() {
        use alexandria_core::store::{AndroidPackageEntry, AndroidRepository, GraphStore};

        let store = GraphStore::open_in_memory().unwrap();
        let repo = AndroidRepository {
            repo_name: "CameraApp".to_string(),
            local_root_path: "/projects/CameraApp".to_string(),
            gradle_root_path: "/projects/CameraApp/application".to_string(),
            gerrit_project: "platform/packages/apps/CameraApp".to_string(),
            primary_app_id: "com.motorola.camera3".to_string(),
            vault_partition: "akashic/com.motorola.camera3".to_string(),
            code_graph_db: "/data/CameraApp.db".to_string(),
            last_indexed_at: 1700000000,
        };
        store.upsert_android_repository(&repo).unwrap();

        let pkg = AndroidPackageEntry {
            package_id: "com.motorola.camera3".to_string(),
            repo_name: "CameraApp".to_string(),
            module_gradle_path: ":app".to_string(),
            role: "application_id".to_string(),
            role_priority: 1,
            flavor_name: "".to_string(),
            is_primary_app: true,
        };
        store.insert_android_package_entries(&[pkg]).unwrap();

        let resolver = AlexandriaSymbolResolver::from_store(store);
        let res = resolver
            .resolve_symbol(
                "com.motorola.camera3.CameraDeviceClient.notifyError",
                Some("com.motorola.camera3"),
            )
            .unwrap()
            .expect("Should resolve symbol");

        assert_eq!(res.repo_name, "CameraApp");
        assert_eq!(res.package_id, "com.motorola.camera3");
        assert_eq!(
            res.symbol,
            "com.motorola.camera3.CameraDeviceClient.notifyError"
        );
    }
}
