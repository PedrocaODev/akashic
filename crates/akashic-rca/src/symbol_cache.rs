use crate::RcaError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

pub const DEFAULT_MAX_MB: u64 = 10_240; // 10 GB

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SymbolArtifactEntry {
    pub key: String,
    pub path: PathBuf,
    pub size_bytes: u64,
    pub last_accessed_ns: u128,
}

#[derive(Debug, Default)]
struct CacheInner {
    entries: HashMap<String, SymbolArtifactEntry>,
    total_size_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct SymbolCache {
    root_dir: PathBuf,
    max_bytes: u64,
    inner: Arc<Mutex<CacheInner>>,
}

fn current_timestamp_nanos() -> u128 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

fn sanitize_key_to_filename(key: &str) -> String {
    key.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_")
}

impl SymbolCache {
    pub fn new(root_dir: impl Into<PathBuf>, max_mb: u64) -> Result<Self, RcaError> {
        let max_bytes = max_mb.saturating_mul(1024 * 1024);
        Self::with_max_bytes(root_dir, max_bytes)
    }

    pub fn with_max_bytes(root_dir: impl Into<PathBuf>, max_bytes: u64) -> Result<Self, RcaError> {
        let root = root_dir.into();
        fs::create_dir_all(&root)?;

        let cache = Self {
            root_dir: root,
            max_bytes,
            inner: Arc::new(Mutex::new(CacheInner::default())),
        };

        cache.scan_existing_artifacts()?;
        Ok(cache)
    }

    pub fn root_dir(&self) -> &Path {
        &self.root_dir
    }

    pub fn max_bytes(&self) -> u64 {
        self.max_bytes
    }

    fn scan_existing_artifacts(&self) -> Result<(), RcaError> {
        if !self.root_dir.is_dir() {
            return Ok(());
        }

        let mut inner = self.inner.lock().map_err(|e| {
            RcaError::Execution(format!("SymbolCache mutex poisoned: {}", e))
        })?;

        for entry in fs::read_dir(&self.root_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                if let Ok(meta) = entry.metadata() {
                    let size = meta.len();
                    let key = entry
                        .file_name()
                        .to_string_lossy()
                        .to_string();
                    let accessed_ns = meta
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
                        .map(|d| d.as_nanos())
                        .unwrap_or_else(current_timestamp_nanos);

                    inner.total_size_bytes = inner.total_size_bytes.saturating_add(size);
                    inner.entries.insert(
                        key.clone(),
                        SymbolArtifactEntry {
                            key,
                            path,
                            size_bytes: size,
                            last_accessed_ns: accessed_ns,
                        },
                    );
                }
            }
        }

        drop(inner);
        self.evict_if_needed()?;
        Ok(())
    }

    pub fn put_artifact(&self, key: &str, bytes: &[u8]) -> Result<PathBuf, RcaError> {
        fs::create_dir_all(&self.root_dir)?;
        let safe_name = sanitize_key_to_filename(key);
        let file_path = self.root_dir.join(&safe_name);

        fs::write(&file_path, bytes)?;
        let size = bytes.len() as u64;
        let now = current_timestamp_nanos();

        {
            let mut inner = self.inner.lock().map_err(|e| {
                RcaError::Execution(format!("SymbolCache mutex poisoned: {}", e))
            })?;

            if let Some(existing) = inner.entries.get(key) {
                inner.total_size_bytes = inner.total_size_bytes.saturating_sub(existing.size_bytes);
            }

            inner.total_size_bytes = inner.total_size_bytes.saturating_add(size);
            inner.entries.insert(
                key.to_string(),
                SymbolArtifactEntry {
                    key: key.to_string(),
                    path: file_path.clone(),
                    size_bytes: size,
                    last_accessed_ns: now,
                },
            );
        }

        self.evict_if_needed()?;
        Ok(file_path)
    }

    pub fn get_artifact(&self, key: &str) -> Option<PathBuf> {
        let mut inner = self.inner.lock().ok()?;
        if let Some(entry) = inner.entries.get_mut(key) {
            if entry.path.exists() {
                entry.last_accessed_ns = current_timestamp_nanos();
                return Some(entry.path.clone());
            } else {
                let size = entry.size_bytes;
                inner.total_size_bytes = inner.total_size_bytes.saturating_sub(size);
                inner.entries.remove(key);
            }
        }
        None
    }

    pub fn total_size_bytes(&self) -> u64 {
        self.inner
            .lock()
            .map(|i| i.total_size_bytes)
            .unwrap_or_default()
    }

    pub fn entry_count(&self) -> usize {
        self.inner
            .lock()
            .map(|i| i.entries.len())
            .unwrap_or_default()
    }

    fn evict_if_needed(&self) -> Result<(), RcaError> {
        let mut inner = self.inner.lock().map_err(|e| {
            RcaError::Execution(format!("SymbolCache mutex poisoned: {}", e))
        })?;

        while inner.total_size_bytes > self.max_bytes && !inner.entries.is_empty() {
            // Find entry with smallest last_accessed_ns
            let oldest_key = inner
                .entries
                .values()
                .min_by_key(|e| e.last_accessed_ns)
                .map(|e| e.key.clone());

            if let Some(key) = oldest_key {
                if let Some(entry) = inner.entries.remove(&key) {
                    inner.total_size_bytes = inner.total_size_bytes.saturating_sub(entry.size_bytes);
                    let _ = fs::remove_file(&entry.path);
                }
            } else {
                break;
            }
        }

        Ok(())
    }

    /// Resolves a native offset using `addr2line`, falling back gracefully to text mappings if absent.
    pub fn resolve_native_offset_with_addr2line(
        &self,
        symbol_key: &str,
        pc_offset: &str,
    ) -> Result<Option<String>, RcaError> {
        let artifact_path = match self.get_artifact(symbol_key) {
            Some(p) => p,
            None => return Ok(None),
        };

        // 1. Try external addr2line binary
        let output = std::process::Command::new("addr2line")
            .arg("-e")
            .arg(&artifact_path)
            .arg("-f")
            .arg("-C")
            .arg(pc_offset)
            .output();

        if let Ok(out) = output {
            if out.status.success() {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let lines: Vec<&str> = stdout.lines().collect();
                if lines.len() >= 2 {
                    let func = lines[0].trim();
                    let loc = lines[1].trim();
                    if func != "??" && !loc.starts_with("??:") {
                        return Ok(Some(format!("{} at {}", func, loc)));
                    }
                } else if !stdout.trim().is_empty() && !stdout.contains("??") {
                    return Ok(Some(stdout.trim().to_string()));
                }
            }
        }

        // 2. Graceful fallback: check if cached artifact is a plaintext symbol table
        // format: "<offset> <symbol> <file:line>" or "<offset> <symbol>"
        if let Ok(content) = fs::read_to_string(&artifact_path) {
            let clean_offset = pc_offset.trim_start_matches("0x").to_ascii_lowercase();
            for line in content.lines() {
                let line_trimmed = line.trim();
                if line_trimmed.is_empty() || line_trimmed.starts_with('#') {
                    continue;
                }
                let parts: Vec<&str> = line_trimmed.split_whitespace().collect();
                if parts.len() >= 2 {
                    let entry_offset = parts[0].trim_start_matches("0x").to_ascii_lowercase();
                    if entry_offset == clean_offset {
                        if parts.len() >= 3 {
                            return Ok(Some(format!("{} at {}", parts[1], parts[2])));
                        } else {
                            return Ok(Some(parts[1].to_string()));
                        }
                    }
                }
            }
        }

        Ok(None)
    }

    /// Retraces an obfuscated line using R8 `retrace` or built-in ProGuard mapping parser fallback.
    pub fn retrace_mapping_line(
        &self,
        mapping_key: &str,
        obf_line: &str,
    ) -> Result<Option<String>, RcaError> {
        let artifact_path = match self.get_artifact(mapping_key) {
            Some(p) => p,
            None => return Ok(None),
        };

        // 1. Try external retrace binary if available
        let output = std::process::Command::new("retrace")
            .arg(&artifact_path)
            .arg(obf_line)
            .output();

        if let Ok(out) = output {
            if out.status.success() {
                let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !stdout.is_empty() && stdout != obf_line.trim() {
                    return Ok(Some(stdout));
                }
            }
        }

        // 2. Built-in deterministic ProGuard / R8 mapping parser fallback
        if let Ok(content) = fs::read_to_string(&artifact_path) {
            if let Some(retraced) = retrace_line_from_mapping(&content, obf_line) {
                return Ok(Some(retraced));
            }
        }

        Ok(None)
    }
}

/// Fallback parser for standard ProGuard / R8 mapping files:
/// "original.pkg.Class -> obf.a.b:"
/// "    int line:line:type originalMethod(...) -> obfMethod"
fn retrace_line_from_mapping(mapping_content: &str, obf_line: &str) -> Option<String> {
    let mut class_mappings: Vec<(&str, &str)> = Vec::new(); // (obf_class, orig_class)

    for line in mapping_content.lines() {
        let trimmed = line.trim();
        if trimmed.ends_with(':') && trimmed.contains(" -> ") {
            let body = trimmed.trim_end_matches(':');
            let parts: Vec<&str> = body.split(" -> ").collect();
            if parts.len() == 2 {
                let orig = parts[0].trim();
                let obf = parts[1].trim();
                class_mappings.push((obf, orig));
            }
        }
    }

    // Sort class mappings by length descending so longer package names match first
    class_mappings.sort_by(|a, b| b.0.len().cmp(&a.0.len()));

    let mut retraced = obf_line.to_string();
    let mut modified = false;

    for (obf_cls, orig_cls) in class_mappings {
        if retraced.contains(obf_cls) {
            retraced = retraced.replace(obf_cls, orig_cls);
            modified = true;
        }
    }

    if modified {
        Some(retraced)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    struct TempDir(PathBuf);
    impl TempDir {
        fn new(prefix: &str) -> Self {
            use std::sync::atomic::{AtomicUsize, Ordering};
            static COUNTER: AtomicUsize = AtomicUsize::new(0);
            let count = COUNTER.fetch_add(1, Ordering::SeqCst);
            let path = std::env::temp_dir().join(format!(
                "{}_{}_{}_{}",
                prefix,
                std::process::id(),
                count,
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir_all(&path).unwrap();
            TempDir(path)
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn test_symbol_cache_put_get_lru() {
        let temp_dir = TempDir::new("sym_cache_lru");
        // 100 bytes max capacity
        let cache = SymbolCache::with_max_bytes(temp_dir.path(), 100).unwrap();

        let b1 = vec![b'a'; 40];
        let b2 = vec![b'b'; 40];
        let b3 = vec![b'c'; 40];

        cache.put_artifact("art1", &b1).unwrap();
        cache.put_artifact("art2", &b2).unwrap();

        assert_eq!(cache.entry_count(), 2);
        assert_eq!(cache.total_size_bytes(), 80);

        // Access art1 so art2 becomes older
        let p1 = cache.get_artifact("art1");
        assert!(p1.is_some());

        // Adding art3 (40 bytes) exceeds 100 bytes (80 + 40 = 120 > 100)
        // art2 should be evicted because art1 was accessed more recently
        cache.put_artifact("art3", &b3).unwrap();

        assert_eq!(cache.entry_count(), 2);
        assert_eq!(cache.total_size_bytes(), 80);

        assert!(cache.get_artifact("art1").is_some());
        assert!(cache.get_artifact("art3").is_some());
        assert!(cache.get_artifact("art2").is_none());
    }

    #[test]
    fn test_retrace_mapping_fallback() {
        let temp_dir = TempDir::new("sym_cache_retrace");
        let cache = SymbolCache::new(temp_dir.path(), 10).unwrap();

        let mapping = "com.motorola.camera3.CameraClient -> a.b.c:\n    void onError() -> a\n";
        cache.put_artifact("mapping.txt", mapping.as_bytes()).unwrap();

        let obf = "    at a.b.c.a(SourceFile:10)";
        let res = cache.retrace_mapping_line("mapping.txt", obf).unwrap();
        assert!(res.is_some());
        let retraced = res.unwrap();
        assert!(retraced.contains("com.motorola.camera3.CameraClient"));
    }

    #[test]
    fn test_resolve_native_offset_fallback() {
        let temp_dir = TempDir::new("sym_cache_offset");
        let cache = SymbolCache::new(temp_dir.path(), 10).unwrap();

        let sym_table = "00045a1c android::CameraClient::notifyError CameraClient.cpp:42\n";
        cache.put_artifact("libcam.sym", sym_table.as_bytes()).unwrap();

        let res = cache
            .resolve_native_offset_with_addr2line("libcam.sym", "0x00045a1c")
            .unwrap();
        assert_eq!(
            res,
            Some("android::CameraClient::notifyError at CameraClient.cpp:42".to_string())
        );
    }
}
