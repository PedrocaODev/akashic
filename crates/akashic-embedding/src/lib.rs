use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EmbeddingError {
    #[error("Embedding provider is offline: {0}")]
    ProviderOffline(String),
    #[error("HTTP error communicating with embedding sidecar: {0}")]
    HttpError(String),
    #[error("Invalid dimensions returned: expected {expected}, actual {actual}")]
    InvalidDimensions { expected: usize, actual: usize },
    #[error("Sidecar error: {0}")]
    SidecarError(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    pub enabled: bool,
    pub endpoint: String,
    pub model: String,
    pub dimensions: usize,
    pub batch_size: usize,
    pub timeout_ms: u64,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            endpoint: "http://127.0.0.1:8080/v1/embeddings".to_string(),
            model: "nomic-embed-text-v1.5".to_string(),
            dimensions: 768,
            batch_size: 32,
            timeout_ms: 5000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmbeddingHealth {
    pub available: bool,
    pub endpoint: String,
    pub model: String,
    pub dimensions: usize,
    pub latency_ms: Option<u64>,
    pub message: String,
}

pub trait EmbeddingProvider: Send + Sync {
    fn dimensions(&self) -> usize;

    fn dimension(&self) -> usize {
        self.dimensions()
    }

    fn is_available(&self) -> bool;

    fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbeddingError>;

    fn embed_one(&self, text: &str) -> Result<Vec<f32>, EmbeddingError> {
        let mut results = self.embed_batch(&[text.to_string()])?;
        results.pop().ok_or_else(|| {
            EmbeddingError::SidecarError("No embedding returned for single input".to_string())
        })
    }

    fn health(&self) -> EmbeddingHealth;
}

pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.is_empty() || b.is_empty() || a.len() != b.len() {
        return 0.0;
    }
    let mut dot = 0.0f32;
    let mut norm_a_sq = 0.0f32;
    let mut norm_b_sq = 0.0f32;
    for (&x, &y) in a.iter().zip(b.iter()) {
        dot += x * y;
        norm_a_sq += x * x;
        norm_b_sq += y * y;
    }
    let norm_a = norm_a_sq.sqrt();
    let norm_b = norm_b_sq.sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    (dot / (norm_a * norm_b)).clamp(-1.0, 1.0)
}

pub fn l2_normalize(v: &mut [f32]) {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
}

pub fn l2_normalized(v: &[f32]) -> Vec<f32> {
    let mut result = v.to_vec();
    l2_normalize(&mut result);
    result
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OfflineEmbeddingProvider {
    pub dimensions: usize,
}

impl OfflineEmbeddingProvider {
    pub fn new(dimensions: usize) -> Self {
        Self { dimensions }
    }
}

impl Default for OfflineEmbeddingProvider {
    fn default() -> Self {
        Self::new(768)
    }
}

impl EmbeddingProvider for OfflineEmbeddingProvider {
    fn dimensions(&self) -> usize {
        self.dimensions
    }

    fn is_available(&self) -> bool {
        false
    }

    fn embed_batch(&self, _texts: &[String]) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        Err(EmbeddingError::ProviderOffline(
            "Offline embedding provider does not support vector generation".to_string(),
        ))
    }

    fn health(&self) -> EmbeddingHealth {
        EmbeddingHealth {
            available: false,
            endpoint: "offline".to_string(),
            model: "none".to_string(),
            dimensions: self.dimensions,
            latency_ms: None,
            message: "Offline provider is active".to_string(),
        }
    }
}

fn fnv1a_64(bytes: &[u8], seed: u64) -> u64 {
    let mut hash = 0xcbf29ce484222325u64 ^ seed;
    for &byte in bytes {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeterministicHashEmbeddingProvider {
    pub dimensions: usize,
}

impl DeterministicHashEmbeddingProvider {
    pub fn new(dimensions: usize) -> Self {
        Self {
            dimensions: dimensions.max(1),
        }
    }

    fn embed_text(&self, text: &str) -> Vec<f32> {
        let mut vec = vec![0.0f32; self.dimensions];
        let lower = text.to_lowercase();

        let mut add_feature = |feature: &[u8], weight: f32| {
            let h1 = fnv1a_64(feature, 0);
            let h2 = fnv1a_64(feature, 0x9e3779b97f4a7c15);
            let bucket = (h1 % (self.dimensions as u64)) as usize;
            let sign = if (h2 & 1) == 0 { 1.0f32 } else { -1.0f32 };
            vec[bucket] += sign * weight;
        };

        for word in lower.split_whitespace() {
            add_feature(word.as_bytes(), 2.0);
        }

        let chars: Vec<char> = lower.chars().collect();
        if chars.len() >= 2 {
            for window in chars.windows(2) {
                let s: String = window.iter().collect();
                add_feature(s.as_bytes(), 1.0);
            }
        }
        if chars.len() >= 3 {
            for window in chars.windows(3) {
                let s: String = window.iter().collect();
                add_feature(s.as_bytes(), 1.0);
            }
        }

        let norm_sq: f32 = vec.iter().map(|x| x * x).sum();
        if norm_sq == 0.0 {
            vec[0] = 1.0;
        } else {
            l2_normalize(&mut vec);
        }

        vec
    }
}

impl Default for DeterministicHashEmbeddingProvider {
    fn default() -> Self {
        Self::new(768)
    }
}

impl EmbeddingProvider for DeterministicHashEmbeddingProvider {
    fn dimensions(&self) -> usize {
        self.dimensions
    }

    fn is_available(&self) -> bool {
        true
    }

    fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        let mut results = Vec::with_capacity(texts.len());
        for text in texts {
            results.push(self.embed_text(text));
        }
        Ok(results)
    }

    fn health(&self) -> EmbeddingHealth {
        EmbeddingHealth {
            available: true,
            endpoint: "local://deterministic-hash".to_string(),
            model: "deterministic-trigram-hash-v1".to_string(),
            dimensions: self.dimensions,
            latency_ms: Some(0),
            message: "Deterministic local hash embedding provider active".to_string(),
        }
    }
}

fn block_on_future<F, T>(fut: F) -> Result<T, EmbeddingError>
where
    F: std::future::Future<Output = Result<T, EmbeddingError>> + Send,
    T: Send,
{
    match tokio::runtime::Handle::try_current() {
        Ok(handle) if handle.runtime_flavor() == tokio::runtime::RuntimeFlavor::MultiThread => {
            tokio::task::block_in_place(|| handle.block_on(fut))
        }
        Ok(_) => std::thread::scope(|s| {
            s.spawn(|| {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|e| EmbeddingError::SidecarError(e.to_string()))?;
                rt.block_on(fut)
            })
            .join()
            .map_err(|_| EmbeddingError::SidecarError("Worker thread panicked".to_string()))?
        }),
        Err(_) => {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| EmbeddingError::SidecarError(e.to_string()))?;
            rt.block_on(fut)
        }
    }
}

#[derive(Debug, Clone)]
pub struct SidecarEmbeddingProvider {
    pub config: EmbeddingConfig,
}

impl SidecarEmbeddingProvider {
    pub fn new(config: EmbeddingConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &EmbeddingConfig {
        &self.config
    }
}

impl Default for SidecarEmbeddingProvider {
    fn default() -> Self {
        Self::new(EmbeddingConfig::default())
    }
}

#[derive(Serialize)]
struct EmbeddingRequest<'a> {
    model: &'a str,
    input: &'a [String],
}

#[derive(Deserialize)]
struct EmbeddingResponseItem {
    embedding: Vec<f32>,
    #[serde(default)]
    index: Option<usize>,
}

#[derive(Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingResponseItem>,
}

impl EmbeddingProvider for SidecarEmbeddingProvider {
    fn dimensions(&self) -> usize {
        self.config.dimensions
    }

    fn is_available(&self) -> bool {
        if !self.config.enabled {
            return false;
        }
        self.health().available
    }

    fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        if !self.config.enabled {
            return Err(EmbeddingError::ProviderOffline(
                "Embedding sidecar is disabled in configuration".to_string(),
            ));
        }

        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let config = self.config.clone();
        let texts = texts.to_vec();

        block_on_future(async move {
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_millis(config.timeout_ms))
                .build()
                .map_err(|e| EmbeddingError::HttpError(e.to_string()))?;

            let batch_size = config.batch_size.max(1);
            let mut all_embeddings = Vec::with_capacity(texts.len());

            for chunk in texts.chunks(batch_size) {
                let body = EmbeddingRequest {
                    model: &config.model,
                    input: chunk,
                };

                let response = client
                    .post(&config.endpoint)
                    .json(&body)
                    .send()
                    .await
                    .map_err(|e| EmbeddingError::HttpError(e.to_string()))?;

                if !response.status().is_success() {
                    return Err(EmbeddingError::SidecarError(format!(
                        "Sidecar returned HTTP status {}",
                        response.status()
                    )));
                }

                let parsed: EmbeddingResponse = response
                    .json()
                    .await
                    .map_err(|e| EmbeddingError::SidecarError(e.to_string()))?;

                let mut items = parsed.data;
                if items.len() != chunk.len() {
                    return Err(EmbeddingError::SidecarError(format!(
                        "Expected {} embeddings, got {}",
                        chunk.len(),
                        items.len()
                    )));
                }

                if items.iter().any(|item| item.index.is_some()) {
                    items.sort_by_key(|item| item.index.unwrap_or(usize::MAX));
                }

                for item in items {
                    if item.embedding.len() != config.dimensions {
                        return Err(EmbeddingError::InvalidDimensions {
                            expected: config.dimensions,
                            actual: item.embedding.len(),
                        });
                    }
                    all_embeddings.push(item.embedding);
                }
            }

            Ok(all_embeddings)
        })
    }

    fn health(&self) -> EmbeddingHealth {
        if !self.config.enabled {
            return EmbeddingHealth {
                available: false,
                endpoint: self.config.endpoint.clone(),
                model: self.config.model.clone(),
                dimensions: self.config.dimensions,
                latency_ms: None,
                message: "Sidecar is disabled in configuration".to_string(),
            };
        }

        let config = self.config.clone();
        let fut = async move {
            let start = std::time::Instant::now();
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_millis(config.timeout_ms))
                .build()
                .map_err(|e| EmbeddingError::HttpError(e.to_string()))?;

            let base = if let Some(idx) = config.endpoint.find("/v1/") {
                &config.endpoint[..idx]
            } else if let Some(scheme_end) = config.endpoint.find("://") {
                let after_scheme = &config.endpoint[scheme_end + 3..];
                if let Some(slash) = after_scheme.find('/') {
                    &config.endpoint[..scheme_end + 3 + slash]
                } else {
                    &config.endpoint
                }
            } else {
                config.endpoint.trim_end_matches('/')
            };

            let health_url = format!("{}/health", base);

            let health_res = client.get(&health_url).send().await;
            match health_res {
                Ok(resp) if resp.status().is_success() => {
                    return Ok(EmbeddingHealth {
                        available: true,
                        endpoint: config.endpoint.clone(),
                        model: config.model.clone(),
                        dimensions: config.dimensions,
                        latency_ms: Some(start.elapsed().as_millis() as u64),
                        message: "Sidecar reachable".to_string(),
                    });
                }
                Ok(resp) if resp.status().as_u16() == 404 || resp.status().as_u16() == 405 => {
                    let models_url = format!("{}/v1/models", base);
                    if let Ok(m_resp) = client.get(&models_url).send().await {
                        if m_resp.status().is_success() {
                            return Ok(EmbeddingHealth {
                                available: true,
                                endpoint: config.endpoint.clone(),
                                model: config.model.clone(),
                                dimensions: config.dimensions,
                                latency_ms: Some(start.elapsed().as_millis() as u64),
                                message: "Sidecar reachable via /v1/models".to_string(),
                            });
                        }
                    }
                    if let Ok(ep_resp) = client.get(&config.endpoint).send().await {
                        if ep_resp.status().is_success() {
                            return Ok(EmbeddingHealth {
                                available: true,
                                endpoint: config.endpoint.clone(),
                                model: config.model.clone(),
                                dimensions: config.dimensions,
                                latency_ms: Some(start.elapsed().as_millis() as u64),
                                message: "Sidecar reachable via endpoint".to_string(),
                            });
                        }
                    }
                    Ok(EmbeddingHealth {
                        available: false,
                        endpoint: config.endpoint.clone(),
                        model: config.model.clone(),
                        dimensions: config.dimensions,
                        latency_ms: None,
                        message: format!("Sidecar endpoint returned HTTP {}", resp.status()),
                    })
                }
                Ok(resp) => {
                    Ok(EmbeddingHealth {
                        available: false,
                        endpoint: config.endpoint.clone(),
                        model: config.model.clone(),
                        dimensions: config.dimensions,
                        latency_ms: None,
                        message: format!("Sidecar returned HTTP {}", resp.status()),
                    })
                }
                Err(e) => {
                    Ok(EmbeddingHealth {
                        available: false,
                        endpoint: config.endpoint.clone(),
                        model: config.model.clone(),
                        dimensions: config.dimensions,
                        latency_ms: None,
                        message: format!("Sidecar unreachable: {e}"),
                    })
                }
            }
        };

        match block_on_future(fut) {
            Ok(health) => health,
            Err(e) => EmbeddingHealth {
                available: false,
                endpoint: self.config.endpoint.clone(),
                model: self.config.model.clone(),
                dimensions: self.config.dimensions,
                latency_ms: None,
                message: format!("Health check error: {e}"),
            },
        }
    }
}

fn is_process_alive(pid: u32) -> bool {
    let stat_path = format!("/proc/{}/stat", pid);
    if let Ok(stat) = std::fs::read_to_string(&stat_path) {
        if let Some(idx) = stat.rfind(')') {
            let rest = stat[idx + 1..].trim_start();
            if let Some(state) = rest.chars().next() {
                return state != 'Z';
            }
        }
        false
    } else {
        std::process::Command::new("kill")
            .arg("-0")
            .arg(pid.to_string())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SidecarSupervisorConfig {
    pub embedding: EmbeddingConfig,
    pub pid_file: std::path::PathBuf,
    pub binary: String,
    pub args: Vec<String>,
    pub startup_timeout_ms: u64,
}

impl Default for SidecarSupervisorConfig {
    fn default() -> Self {
        Self {
            embedding: EmbeddingConfig::default(),
            pid_file: std::path::PathBuf::from(".akashic/embedding-sidecar.pid"),
            binary: "llama-server".to_string(),
            args: vec![
                "--embedding".to_string(),
                "--port".to_string(),
                "8080".to_string(),
            ],
            startup_timeout_ms: 2000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SidecarStatus {
    pub running: bool,
    pub pid: Option<u32>,
    pub pid_file: std::path::PathBuf,
    pub health: EmbeddingHealth,
}

#[derive(Debug, Clone)]
pub struct SidecarSupervisor {
    config: SidecarSupervisorConfig,
}

impl SidecarSupervisor {
    pub fn new(config: SidecarSupervisorConfig) -> Self {
        Self { config }
    }

    pub fn with_pid_file(
        embedding: EmbeddingConfig,
        pid_file: impl Into<std::path::PathBuf>,
    ) -> Self {
        Self {
            config: SidecarSupervisorConfig {
                embedding,
                pid_file: pid_file.into(),
                ..SidecarSupervisorConfig::default()
            },
        }
    }

    pub fn config(&self) -> &SidecarSupervisorConfig {
        &self.config
    }

    pub fn provider(&self) -> SidecarEmbeddingProvider {
        SidecarEmbeddingProvider::new(self.config.embedding.clone())
    }

    pub fn status(&self) -> SidecarStatus {
        let mut pid = None;
        let mut pid_alive = false;

        if self.config.pid_file.exists() {
            if let Ok(content) = std::fs::read_to_string(&self.config.pid_file) {
                if let Ok(parsed_pid) = content.trim().parse::<u32>() {
                    if is_process_alive(parsed_pid) {
                        pid = Some(parsed_pid);
                        pid_alive = true;
                    } else {
                        let _ = std::fs::remove_file(&self.config.pid_file);
                    }
                } else {
                    let _ = std::fs::remove_file(&self.config.pid_file);
                }
            }
        }

        let health = self.provider().health();
        let running = pid_alive || health.available;

        SidecarStatus {
            running,
            pid,
            pid_file: self.config.pid_file.clone(),
            health,
        }
    }

    pub fn start(&self) -> Result<SidecarStatus, EmbeddingError> {
        let current = self.status();
        if current.running {
            return Ok(current);
        }

        if let Some(parent) = self.config.pid_file.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    EmbeddingError::SidecarError(format!("Failed to create PID file dir: {e}"))
                })?;
            }
        }

        let child = std::process::Command::new(&self.config.binary)
            .args(&self.config.args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| {
                EmbeddingError::SidecarError(format!(
                    "Failed to spawn {}: {e}",
                    self.config.binary
                ))
            })?;

        let pid = child.id();
        std::fs::write(&self.config.pid_file, pid.to_string()).map_err(|e| {
            EmbeddingError::SidecarError(format!("Failed to write PID file: {e}"))
        })?;

        if self.config.embedding.enabled && self.config.startup_timeout_ms > 0 {
            let start = std::time::Instant::now();
            let timeout = std::time::Duration::from_millis(self.config.startup_timeout_ms);
            let poll_interval = std::time::Duration::from_millis(50);
            while start.elapsed() < timeout {
                let health = self.provider().health();
                if health.available {
                    break;
                }
                std::thread::sleep(poll_interval);
            }
        }

        Ok(self.status())
    }

    pub fn stop(&self) -> Result<SidecarStatus, EmbeddingError> {
        if self.config.pid_file.exists() {
            if let Ok(content) = std::fs::read_to_string(&self.config.pid_file) {
                if let Ok(pid) = content.trim().parse::<u32>() {
                    let _ = std::process::Command::new("kill")
                        .arg("-TERM")
                        .arg(pid.to_string())
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .status();

                    let mut stopped = false;
                    for _ in 0..10 {
                        std::thread::sleep(std::time::Duration::from_millis(50));
                        if !is_process_alive(pid) {
                            stopped = true;
                            break;
                        }
                    }

                    if !stopped {
                        let _ = std::process::Command::new("kill")
                            .arg("-KILL")
                            .arg(pid.to_string())
                            .stdout(std::process::Stdio::null())
                            .stderr(std::process::Stdio::null())
                            .status();
                        std::thread::sleep(std::time::Duration::from_millis(50));
                    }
                }
            }
            let _ = std::fs::remove_file(&self.config.pid_file);
        }

        Ok(self.status())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    #[test]
    fn test_default_config() {
        let config = EmbeddingConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.dimensions, 768);
        assert_eq!(config.endpoint, "http://127.0.0.1:8080/v1/embeddings");
        assert_eq!(config.model, "nomic-embed-text-v1.5");
        assert_eq!(config.batch_size, 32);
        assert_eq!(config.timeout_ms, 5000);
    }

    #[test]
    fn test_offline_provider() {
        let provider = OfflineEmbeddingProvider::default();
        assert_eq!(provider.dimensions(), 768);
        assert_eq!(provider.dimension(), 768);
        assert!(!provider.is_available());
        let health = provider.health();
        assert!(!health.available);
        assert!(matches!(
            provider.embed_batch(&["hello".to_string()]),
            Err(EmbeddingError::ProviderOffline(_))
        ));
        assert!(matches!(
            provider.embed_one("hello"),
            Err(EmbeddingError::ProviderOffline(_))
        ));
    }

    #[test]
    fn test_cosine_similarity() {
        // Identical vectors -> 1.0
        let v = vec![1.0, 2.0, 3.0];
        assert!((cosine_similarity(&v, &v) - 1.0).abs() < 1e-6);

        // Orthogonal vectors -> 0.0
        assert_eq!(cosine_similarity(&[1.0, 0.0], &[0.0, 1.0]), 0.0);

        // Opposite vectors -> -1.0
        assert!((cosine_similarity(&[1.0, 0.0], &[-1.0, 0.0]) - (-1.0)).abs() < 1e-6);

        // Unnormalized scaled vectors -> 1.0
        assert!((cosine_similarity(&[1.0, 2.0], &[2.0, 4.0]) - 1.0).abs() < 1e-6);

        // Empty or mismatched slices -> 0.0
        assert_eq!(cosine_similarity(&[], &[]), 0.0);
        assert_eq!(cosine_similarity(&[1.0], &[1.0, 2.0]), 0.0);
        assert_eq!(cosine_similarity(&[0.0, 0.0], &[1.0, 1.0]), 0.0);
    }

    #[test]
    fn test_l2_normalization() {
        let mut v = vec![3.0, 4.0];
        l2_normalize(&mut v);
        assert!((v[0] - 0.6).abs() < 1e-6);
        assert!((v[1] - 0.8).abs() < 1e-6);

        let v_norm = l2_normalized(&[0.0, 5.0]);
        assert_eq!(v_norm, vec![0.0, 1.0]);
    }

    #[test]
    fn test_deterministic_hash_provider() {
        let p768 = DeterministicHashEmbeddingProvider::default();
        let p64 = DeterministicHashEmbeddingProvider::new(64);

        assert_eq!(p768.dimensions(), 768);
        assert_eq!(p768.dimension(), 768);
        assert_eq!(p64.dimensions(), 64);
        assert!(p768.is_available());

        let health = p768.health();
        assert!(health.available);
        assert_eq!(health.endpoint, "local://deterministic-hash");
        assert_eq!(health.model, "deterministic-trigram-hash-v1");
        assert_eq!(health.dimensions, 768);

        // Batch embedding dimensions
        let em768 = p768
            .embed_batch(&["hello".to_string(), "world".to_string()])
            .unwrap();
        assert_eq!(em768.len(), 2);
        assert_eq!(em768[0].len(), 768);
        assert_eq!(em768[1].len(), 768);

        let em64 = p64.embed_one("hello").unwrap();
        assert_eq!(em64.len(), 64);

        // L2 normalization: Euclidean norm ~ 1.0
        let norm = em64.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5);

        // Empty string handling (norm 1.0, vec[0] = 1.0)
        let em_empty = p64.embed_one("").unwrap();
        let norm_empty = em_empty.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm_empty - 1.0).abs() < 1e-5);
        assert_eq!(em_empty[0], 1.0);

        // Determinism across calls
        let em1 = p768.embed_one("deterministic test").unwrap();
        let em2 = p768.embed_one("deterministic test").unwrap();
        assert_eq!(em1, em2);

        // Cosine similarity: similar strings (> 0.5) vs dissimilar strings (< 0.3)
        let s1 = "The quick brown fox jumps over the lazy dog";
        let s2 = "The quick brown fox leaps over the lazy dog";
        let s3 = "Quantum computing uses qubits and wave function superposition";
        let v1 = p768.embed_one(s1).unwrap();
        let v2 = p768.embed_one(s2).unwrap();
        let v3 = p768.embed_one(s3).unwrap();
        let sim_12 = cosine_similarity(&v1, &v2);
        let sim_13 = cosine_similarity(&v1, &v3);
        assert!(sim_12 > 0.5, "Expected > 0.5, got {}", sim_12);
        assert!(sim_13 < 0.3, "Expected < 0.3, got {}", sim_13);
    }

    #[test]
    fn test_sidecar_provider_disabled() {
        let provider = SidecarEmbeddingProvider::default();
        assert_eq!(provider.dimensions(), 768);
        assert_eq!(provider.dimension(), 768);
        assert!(!provider.is_available());
        let health = provider.health();
        assert!(!health.available);
        assert!(matches!(
            provider.embed_batch(&["hello".to_string()]),
            Err(EmbeddingError::ProviderOffline(_))
        ));
    }

    #[test]
    fn test_sidecar_provider_mock_batch_chunking() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();

        let handle = std::thread::spawn(move || {
            // Expect 2 requests because batch_size = 2 with 3 input items
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut buf = [0u8; 4096];
                let n = stream.read(&mut buf).unwrap();
                let req = String::from_utf8_lossy(&buf[..n]);

                let body = if req.contains("\"input\":[\"item 1\",\"item 2\"]")
                    || req.contains("\"item 1\"")
                {
                    r#"{"data":[{"embedding":[1.0,0.0,0.0,0.0],"index":0},{"embedding":[0.0,1.0,0.0,0.0],"index":1}]}"#
                } else {
                    r#"{"data":[{"embedding":[0.0,0.0,1.0,0.0],"index":0}]}"#
                };

                let resp = format!(
                    "HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                    body.len(),
                    body
                );
                stream.write_all(resp.as_bytes()).unwrap();
            }
        });

        let config = EmbeddingConfig {
            enabled: true,
            endpoint: format!("http://127.0.0.1:{port}/v1/embeddings"),
            model: "test-model".to_string(),
            dimensions: 4,
            batch_size: 2,
            timeout_ms: 2000,
        };
        let provider = SidecarEmbeddingProvider::new(config);

        let texts = vec![
            "item 1".to_string(),
            "item 2".to_string(),
            "item 3".to_string(),
        ];
        let embeddings = provider.embed_batch(&texts).expect("embed_batch should succeed");
        assert_eq!(embeddings.len(), 3);
        assert_eq!(embeddings[0], vec![1.0, 0.0, 0.0, 0.0]);
        assert_eq!(embeddings[1], vec![0.0, 1.0, 0.0, 0.0]);
        assert_eq!(embeddings[2], vec![0.0, 0.0, 1.0, 0.0]);

        handle.join().unwrap();
    }

    #[test]
    fn test_sidecar_provider_mock_dimension_mismatch() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();

        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf).unwrap();
            let body = r#"{"data":[{"embedding":[1.0,0.0],"index":0}]}"#;
            let resp = format!(
                "HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(resp.as_bytes()).unwrap();
        });

        let config = EmbeddingConfig {
            enabled: true,
            endpoint: format!("http://127.0.0.1:{port}/v1/embeddings"),
            model: "test-model".to_string(),
            dimensions: 4,
            batch_size: 2,
            timeout_ms: 2000,
        };
        let provider = SidecarEmbeddingProvider::new(config);

        let res = provider.embed_batch(&["item 1".to_string()]);
        assert!(matches!(
            res,
            Err(EmbeddingError::InvalidDimensions {
                expected: 4,
                actual: 2
            })
        ));

        handle.join().unwrap();
    }

    #[test]
    fn test_sidecar_provider_mock_health() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();

        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf).unwrap();
            let resp = "HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Length: 2\r\n\r\n{}";
            stream.write_all(resp.as_bytes()).unwrap();
        });

        let config = EmbeddingConfig {
            enabled: true,
            endpoint: format!("http://127.0.0.1:{port}/v1/embeddings"),
            model: "test-model".to_string(),
            dimensions: 768,
            batch_size: 32,
            timeout_ms: 2000,
        };
        let provider = SidecarEmbeddingProvider::new(config);

        let health = provider.health();
        assert!(health.available);
        assert!(health.latency_ms.is_some());
        assert_eq!(health.dimensions, 768);

        handle.join().unwrap();
    }

    #[test]
    fn test_sidecar_provider_offline_fallback() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);

        let config = EmbeddingConfig {
            enabled: true,
            endpoint: format!("http://127.0.0.1:{port}/v1/embeddings"),
            model: "test-model".to_string(),
            dimensions: 768,
            batch_size: 32,
            timeout_ms: 500,
        };
        let provider = SidecarEmbeddingProvider::new(config);

        assert!(!provider.is_available());
        let health = provider.health();
        assert!(!health.available);
        assert!(health.latency_ms.is_none());
        assert!(matches!(
            provider.embed_batch(&["test".to_string()]),
            Err(EmbeddingError::HttpError(_)) | Err(EmbeddingError::ProviderOffline(_))
        ));
    }

    #[test]
    fn test_sidecar_supervisor_lifecycle() {
        let pid_path = std::env::temp_dir().join(format!(
            "test-embedding-sidecar-{}-{}.pid",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));

        let config = SidecarSupervisorConfig {
            embedding: EmbeddingConfig::default(),
            pid_file: pid_path.clone(),
            binary: "sleep".to_string(),
            args: vec!["30".to_string()],
            startup_timeout_ms: 100,
        };

        let supervisor = SidecarSupervisor::new(config);
        assert_eq!(supervisor.config().binary, "sleep");
        assert_eq!(supervisor.provider().dimensions(), 768);

        let start_status = supervisor.start().expect("supervisor start failed");
        assert!(start_status.running);
        assert!(start_status.pid.is_some());
        assert!(pid_path.exists());

        let status = supervisor.status();
        assert!(status.running);
        assert_eq!(status.pid, start_status.pid);

        let stop_status = supervisor.stop().expect("supervisor stop failed");
        assert!(!stop_status.running);
        assert!(stop_status.pid.is_none());
        assert!(!pid_path.exists());
    }
}
