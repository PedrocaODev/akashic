use crate::symbol_cache::SymbolCache;
use crate::RcaError;
use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;

pub type CodeGraphStore = akashic_engine::GraphStore;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FrameKind {
    JavaKotlin,
    Native,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StackFrame {
    pub frame_index: usize,
    pub frame_kind: FrameKind,
    pub instruction_addr: u64,
    pub module_name: Option<String>,
    pub raw_symbol: Option<String>,
    pub class_name: Option<String>,
    pub method_name: Option<String>,
    pub file: Option<String>,
    pub line: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedRepoTarget {
    pub repo_name: String,
    pub package_id: String,
    pub file: Option<String>,
    pub line: Option<u32>,
    pub symbol: String,
    pub class_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SymbolicationResult {
    pub crash_signature: String,
    pub is_native_crash: bool,
    pub exception_or_signal: String,
    pub symbolicated_stack_trace: String,
    pub faulting_symbol: Option<ResolvedRepoTarget>,
    pub frames: Vec<StackFrame>,
    pub evidence_payload: serde_json::Value,
}

pub trait SymbolResolver: Send + Sync {
    fn resolve_symbol(
        &self,
        raw_symbol: &str,
        crash_app_id: Option<&str>,
    ) -> Result<Option<ResolvedRepoTarget>, RcaError>;
}

#[derive(Debug, Clone, Default)]
pub struct InMemorySymbolResolver {
    symbols_by_app: HashMap<(String, String), ResolvedRepoTarget>,
    symbols_generic: HashMap<String, ResolvedRepoTarget>,
}

impl InMemorySymbolResolver {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_mapping(
        &mut self,
        raw_symbol: impl Into<String>,
        crash_app_id: Option<impl Into<String>>,
        resolved: ResolvedRepoTarget,
    ) {
        let sym = raw_symbol.into();
        if let Some(app) = crash_app_id {
            self.symbols_by_app
                .insert((sym.clone(), app.into()), resolved.clone());
        }
        self.symbols_generic.insert(sym, resolved);
    }

    pub fn with_motorola_defaults() -> Self {
        let mut resolver = Self::new();
        resolver.add_mapping(
            "com.motorola.camera3.CameraDeviceClient.notifyError",
            Some("com.motorola.camera3"),
            ResolvedRepoTarget {
                repo_name: "motorola-camera".to_string(),
                package_id: "com.motorola.camera3".to_string(),
                file: Some("src/com/motorola/camera3/CameraDeviceClient.java".to_string()),
                line: Some(412),
                symbol: "com.motorola.camera3.CameraDeviceClient.notifyError".to_string(),
                class_name: Some("com.motorola.camera3.CameraDeviceClient".to_string()),
            },
        );
        resolver.add_mapping(
            "com.motorola.camera3.feature.Mode$1.execute",
            Some("com.motorola.camera3"),
            ResolvedRepoTarget {
                repo_name: "motorola-camera".to_string(),
                package_id: "com.motorola.camera3".to_string(),
                file: Some("src/com/motorola/camera3/feature/Mode.kt".to_string()),
                line: Some(88),
                symbol: "com.motorola.camera3.feature.Mode$1.execute".to_string(),
                class_name: Some("com.motorola.camera3.feature.Mode$1".to_string()),
            },
        );
        resolver.add_mapping(
            "android::CameraDeviceClient::notifyError",
            None::<String>,
            ResolvedRepoTarget {
                repo_name: "frameworks-av".to_string(),
                package_id: "android.hardware.camera2".to_string(),
                file: Some("services/camera/libcameraservice/CameraDeviceClient.cpp".to_string()),
                line: Some(512),
                symbol: "android::CameraDeviceClient::notifyError".to_string(),
                class_name: Some("android::CameraDeviceClient".to_string()),
            },
        );
        resolver
    }
}

impl SymbolResolver for InMemorySymbolResolver {
    fn resolve_symbol(
        &self,
        raw_symbol: &str,
        crash_app_id: Option<&str>,
    ) -> Result<Option<ResolvedRepoTarget>, RcaError> {
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

pub struct EngineSymbolResolver {
    store: Arc<std::sync::Mutex<akashic_engine::GraphStore>>,
    fallback: Option<InMemorySymbolResolver>,
}

impl EngineSymbolResolver {
    pub fn new(store: akashic_engine::GraphStore) -> Self {
        Self {
            store: Arc::new(std::sync::Mutex::new(store)),
            fallback: None,
        }
    }

    pub fn from_arc(store: Arc<std::sync::Mutex<akashic_engine::GraphStore>>) -> Self {
        Self {
            store,
            fallback: None,
        }
    }

    pub fn with_fallback(mut self, fallback: InMemorySymbolResolver) -> Self {
        self.fallback = Some(fallback);
        self
    }

    pub fn with_motorola_defaults(store: akashic_engine::GraphStore) -> Self {
        Self::new(store).with_fallback(InMemorySymbolResolver::with_motorola_defaults())
    }
}

impl SymbolResolver for EngineSymbolResolver {
    fn resolve_symbol(
        &self,
        raw_symbol: &str,
        crash_app_id: Option<&str>,
    ) -> Result<Option<ResolvedRepoTarget>, RcaError> {
        let store = self
            .store
            .lock()
            .map_err(|e| RcaError::Execution(format!("Mutex poisoned: {}", e)))?;
        let res = store
            .resolve_stack_symbol(raw_symbol, crash_app_id)
            .map_err(|e| RcaError::Execution(format!("Engine error: {}", e)))?;

        if let Some(pkg) = res {
            return Ok(Some(ResolvedRepoTarget {
                repo_name: pkg.repo_name,
                package_id: pkg.package_id,
                file: None,
                line: None,
                symbol: raw_symbol.to_string(),
                class_name: None,
            }));
        }

        if let Some(ref fb) = self.fallback {
            return fb.resolve_symbol(raw_symbol, crash_app_id);
        }

        Ok(None)
    }
}

pub fn parse_stack_trace(raw_log: &str) -> (String, Vec<StackFrame>) {
    let is_native = raw_log.lines().any(|l| {
        let t = l.trim_start();
        t.starts_with("#00 pc ")
            || t.starts_with("#0 pc ")
            || t.starts_with("backtrace:")
            || t.contains("BuildId:")
            || t.contains("Fatal signal ")
    });

    let signal_regex = Regex::new(r#"(?i)signal\s+\d+\s*\((SIG[A-Z]+)\)|(?i)\b(SIG[A-Z]+)\b"#)
        .expect("valid signal regex");
    let java_exception_regex = Regex::new(r#"([a-zA-Z0-9_$.]+(?:Exception|Error)):?"#)
        .expect("valid java exception regex");

    let exception_or_signal = if is_native {
        if let Some(caps) = signal_regex.captures(raw_log) {
            caps.get(1)
                .or_else(|| caps.get(2))
                .map(|m| m.as_str().to_string())
                .unwrap_or_else(|| "SIGSEGV".to_string())
        } else {
            "SIGSEGV".to_string()
        }
    } else if let Some(caps) = java_exception_regex.captures(raw_log) {
        caps.get(1)
            .map(|m| m.as_str().to_string())
            .unwrap_or_else(|| "java.lang.RuntimeException".to_string())
    } else {
        "FATAL_EXCEPTION".to_string()
    };

    let mut frames = Vec::new();
    let mut frame_index = 0;

    if is_native {
        let native_frame_regex = Regex::new(
            r#"^\s*#(\d+)\s+pc\s+([0-9a-fA-F]+)\s+(\S+)(?:\s+\((.+?)(?:\+(\d+))?\))?(?:\s+\(BuildId:.*\))?"#,
        )
        .expect("valid native frame regex");

        for line in raw_log.lines() {
            if let Some(caps) = native_frame_regex.captures(line) {
                let parsed_idx: usize = caps
                    .get(1)
                    .and_then(|m| m.as_str().parse().ok())
                    .unwrap_or(frame_index);
                let addr = caps
                    .get(2)
                    .and_then(|m| u64::from_str_radix(m.as_str(), 16).ok())
                    .unwrap_or(0);
                let module = caps.get(3).map(|m| m.as_str().to_string());
                let raw_sym = caps.get(4).map(|m| m.as_str().to_string());

                let class_name = if let Some(ref sym) = raw_sym {
                    if sym.contains("::") {
                        let parts: Vec<&str> = sym.rsplitn(2, "::").collect();
                        if parts.len() == 2 {
                            Some(parts[1].to_string())
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                };

                frames.push(StackFrame {
                    frame_index: parsed_idx,
                    frame_kind: FrameKind::Native,
                    instruction_addr: addr,
                    module_name: module,
                    raw_symbol: raw_sym,
                    class_name,
                    method_name: None,
                    file: None,
                    line: None,
                });
                frame_index += 1;
            }
        }
    } else {
        let java_frame_regex = Regex::new(
            r#"^\s*at\s+([a-zA-Z0-9_$]+(?:\.[a-zA-Z0-9_$]+)+)\.([a-zA-Z0-9_$<>]+)\((?:([a-zA-Z0-9_$-]+\.(?:java|kt|scala)):(\d+)|([^)]+))\)"#,
        )
        .expect("valid java frame regex");

        for line in raw_log.lines() {
            if let Some(caps) = java_frame_regex.captures(line) {
                let class_name = caps.get(1).map(|m| m.as_str().to_string()).unwrap_or_default();
                let method_name = caps.get(2).map(|m| m.as_str().to_string()).unwrap_or_default();
                let full_sym = format!("{}.{}", class_name, method_name);
                let file = caps.get(3).map(|m| m.as_str().to_string());
                let line_num = caps.get(4).and_then(|m| m.as_str().parse::<u32>().ok());

                frames.push(StackFrame {
                    frame_index,
                    frame_kind: FrameKind::JavaKotlin,
                    instruction_addr: 0,
                    module_name: None,
                    raw_symbol: Some(full_sym),
                    class_name: Some(class_name),
                    method_name: Some(method_name),
                    file,
                    line: line_num,
                });
                frame_index += 1;
            }
        }
    }

    (exception_or_signal, frames)
}

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

pub fn symbolicate_crash(
    app_id: &str,
    raw_log: &str,
    resolver: &dyn SymbolResolver,
) -> Result<SymbolicationResult, RcaError> {
    let (exception_or_signal, mut frames) = parse_stack_trace(raw_log);
    let is_native = frames
        .first()
        .map(|f| f.frame_kind == FrameKind::Native)
        .unwrap_or(false);

    let mut faulting_symbol: Option<ResolvedRepoTarget> = None;

    for frame in &mut frames {
        if let Some(ref sym) = frame.raw_symbol {
            if let Some(res) = resolver.resolve_symbol(sym, Some(app_id))? {
                if res.file.is_some() {
                    frame.file = res.file.clone();
                }
                if frame.line.is_none() && res.line.is_some() {
                    frame.line = res.line;
                }
                if frame.class_name.is_none() && res.class_name.is_some() {
                    frame.class_name = res.class_name.clone();
                }
                if faulting_symbol.is_none() {
                    faulting_symbol = Some(ResolvedRepoTarget {
                        repo_name: res.repo_name,
                        package_id: res.package_id,
                        file: frame.file.clone(),
                        line: frame.line,
                        symbol: sym.clone(),
                        class_name: frame.class_name.clone(),
                    });
                }
            }
        }
    }

    // Top 3 frames for crash signature
    let top_frames: Vec<String> = frames
        .iter()
        .take(3)
        .map(|f| {
            f.raw_symbol
                .clone()
                .or_else(|| f.module_name.clone())
                .unwrap_or_default()
        })
        .collect();

    let crash_signature = compute_crash_signature(app_id, &exception_or_signal, &top_frames);

    let mut stack_trace_lines = Vec::new();
    if !is_native {
        stack_trace_lines.push(format!("{}: fault at top frame", exception_or_signal));
        for f in &frames {
            let loc = match (&f.file, f.line) {
                (Some(file), Some(line)) => format!("{}:{}", file, line),
                (Some(file), None) => file.clone(),
                _ => "Unknown Source".to_string(),
            };
            let sym = f.raw_symbol.as_deref().unwrap_or("unknown");
            stack_trace_lines.push(format!("    at {}({})", sym, loc));
        }
    } else {
        stack_trace_lines.push("backtrace:".to_string());
        for f in &frames {
            let loc = match (&f.file, f.line) {
                (Some(file), Some(line)) => format!(" ({}:{})", file, line),
                _ => String::new(),
            };
            let sym = f
                .raw_symbol
                .as_deref()
                .or(f.module_name.as_deref())
                .unwrap_or("unknown");
            stack_trace_lines.push(format!(
                "    #{:02} pc {:08x} {}{}",
                f.frame_index, f.instruction_addr, sym, loc
            ));
        }
    }

    let symbolicated_stack_trace = stack_trace_lines.join("\n");

    let evidence_payload = serde_json::json!({
        "crash_signature": crash_signature,
        "is_native_crash": is_native,
        "exception_or_signal": exception_or_signal,
        "app_id": app_id,
        "faulting_symbol": faulting_symbol,
        "frames": frames,
    });

    Ok(SymbolicationResult {
        crash_signature,
        is_native_crash: is_native,
        exception_or_signal,
        symbolicated_stack_trace,
        faulting_symbol,
        frames,
        evidence_payload,
    })
}

pub fn symbolicate_with_cache(
    app_id: &str,
    raw_log: &str,
    resolver: &dyn SymbolResolver,
    cache: &SymbolCache,
) -> Result<SymbolicationResult, RcaError> {
    let (exception_or_signal, mut frames) = parse_stack_trace(raw_log);
    let is_native = frames
        .first()
        .map(|f| f.frame_kind == FrameKind::Native)
        .unwrap_or(false);

    let mut faulting_symbol: Option<ResolvedRepoTarget> = None;

    for frame in &mut frames {
        // If native, attempt addr2line cache lookup if module exists
        if frame.frame_kind == FrameKind::Native && frame.raw_symbol.is_none() {
            if let Some(ref mod_name) = frame.module_name {
                let pc = format!("0x{:08x}", frame.instruction_addr);
                let clean_mod = mod_name.rsplit('/').next().unwrap_or(mod_name);
                if let Ok(Some(resolved_text)) =
                    cache.resolve_native_offset_with_addr2line(clean_mod, &pc)
                {
                    frame.raw_symbol = Some(resolved_text);
                }
            }
        }

        if let Some(ref sym) = frame.raw_symbol {
            if let Some(res) = resolver.resolve_symbol(sym, Some(app_id))? {
                if res.file.is_some() {
                    frame.file = res.file.clone();
                }
                if frame.line.is_none() && res.line.is_some() {
                    frame.line = res.line;
                }
                if frame.class_name.is_none() && res.class_name.is_some() {
                    frame.class_name = res.class_name.clone();
                }
                if faulting_symbol.is_none() {
                    faulting_symbol = Some(ResolvedRepoTarget {
                        repo_name: res.repo_name,
                        package_id: res.package_id,
                        file: frame.file.clone(),
                        line: frame.line,
                        symbol: sym.clone(),
                        class_name: frame.class_name.clone(),
                    });
                }
            }
        }
    }

    let top_frames: Vec<String> = frames
        .iter()
        .take(3)
        .map(|f| {
            f.raw_symbol
                .clone()
                .or_else(|| f.module_name.clone())
                .unwrap_or_default()
        })
        .collect();

    let crash_signature = compute_crash_signature(app_id, &exception_or_signal, &top_frames);

    let mut stack_trace_lines = Vec::new();
    if !is_native {
        stack_trace_lines.push(format!("{}: fault at top frame", exception_or_signal));
        for f in &frames {
            let loc = match (&f.file, f.line) {
                (Some(file), Some(line)) => format!("{}:{}", file, line),
                (Some(file), None) => file.clone(),
                _ => "Unknown Source".to_string(),
            };
            let sym = f.raw_symbol.as_deref().unwrap_or("unknown");
            stack_trace_lines.push(format!("    at {}({})", sym, loc));
        }
    } else {
        stack_trace_lines.push("backtrace:".to_string());
        for f in &frames {
            let loc = match (&f.file, f.line) {
                (Some(file), Some(line)) => format!(" ({}:{})", file, line),
                _ => String::new(),
            };
            let sym = f
                .raw_symbol
                .as_deref()
                .or(f.module_name.as_deref())
                .unwrap_or("unknown");
            stack_trace_lines.push(format!(
                "    #{:02} pc {:08x} {}{}",
                f.frame_index, f.instruction_addr, sym, loc
            ));
        }
    }

    let symbolicated_stack_trace = stack_trace_lines.join("\n");

    let evidence_payload = serde_json::json!({
        "crash_signature": crash_signature,
        "is_native_crash": is_native,
        "exception_or_signal": exception_or_signal,
        "app_id": app_id,
        "faulting_symbol": faulting_symbol,
        "frames": frames,
    });

    Ok(SymbolicationResult {
        crash_signature,
        is_native_crash: is_native,
        exception_or_signal,
        symbolicated_stack_trace,
        faulting_symbol,
        frames,
        evidence_payload,
    })
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

        let s1 = compute_crash_signature(app, ex, &frames);
        let s2 = compute_crash_signature(app, ex, &frames);
        assert_eq!(s1, s2);
        assert_eq!(s1.len(), 64);
    }

    #[test]
    fn test_parse_and_symbolicate_java() {
        let raw = r#"
java.lang.NullPointerException: Attempt to read from null object
    at com.motorola.camera3.CameraDeviceClient.notifyError(CameraDeviceClient.java:412)
    at com.motorola.camera3.feature.Mode$1.execute(Mode.kt:88)
"#;
        let resolver = InMemorySymbolResolver::with_motorola_defaults();
        let res = symbolicate_crash("com.motorola.camera3", raw, &resolver).unwrap();

        assert!(!res.is_native_crash);
        assert_eq!(res.exception_or_signal, "java.lang.NullPointerException");
        assert_eq!(res.frames.len(), 2);

        let fault = res.faulting_symbol.unwrap();
        assert_eq!(fault.repo_name, "motorola-camera");
        assert_eq!(fault.file, Some("src/com/motorola/camera3/CameraDeviceClient.java".to_string()));
        assert_eq!(fault.line, Some(412));
    }

    #[test]
    fn test_parse_and_symbolicate_native() {
        let raw = r#"
Fatal signal 11 (SIGSEGV), code 1 (SEGV_MAPERR), fault addr 0x0
backtrace:
    #00 pc 00042f10 /system/lib64/libcamera_client.so (android::CameraDeviceClient::notifyError+12)
"#;
        let resolver = InMemorySymbolResolver::with_motorola_defaults();
        let res = symbolicate_crash("com.motorola.camera3", raw, &resolver).unwrap();

        assert!(res.is_native_crash);
        assert_eq!(res.exception_or_signal, "SIGSEGV");
        assert_eq!(res.frames.len(), 1);
        assert_eq!(res.frames[0].instruction_addr, 0x00042f10);

        let fault = res.faulting_symbol.unwrap();
        assert_eq!(fault.repo_name, "frameworks-av");
        assert_eq!(fault.line, Some(512));
    }
}
