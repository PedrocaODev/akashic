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
    fn is_available(&self) -> bool;
    fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbeddingError>;
    fn health(&self) -> EmbeddingHealth;
}

#[derive(Debug, Clone)]
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

        let run_request = async {
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_millis(self.config.timeout_ms))
                .build()
                .map_err(|e| EmbeddingError::HttpError(e.to_string()))?;

            let body = EmbeddingRequest {
                model: &self.config.model,
                input: texts,
            };

            let response = client
                .post(&self.config.endpoint)
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

            let mut embeddings = Vec::with_capacity(parsed.data.len());
            for item in parsed.data {
                if item.embedding.len() != self.config.dimensions {
                    return Err(EmbeddingError::InvalidDimensions {
                        expected: self.config.dimensions,
                        actual: item.embedding.len(),
                    });
                }
                embeddings.push(item.embedding);
            }

            Ok(embeddings)
        };

        match tokio::runtime::Handle::try_current() {
            Ok(handle) => tokio::task::block_in_place(|| handle.block_on(run_request)),
            Err(_) => {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|e| EmbeddingError::SidecarError(e.to_string()))?;
                rt.block_on(run_request)
            }
        }
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

        let check = async {
            let start = std::time::Instant::now();
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_millis(self.config.timeout_ms))
                .build();

            let client = match client {
                Ok(c) => c,
                Err(e) => {
                    return EmbeddingHealth {
                        available: false,
                        endpoint: self.config.endpoint.clone(),
                        model: self.config.model.clone(),
                        dimensions: self.config.dimensions,
                        latency_ms: None,
                        message: format!("Failed to build client: {e}"),
                    }
                }
            };

            match client.get(&self.config.endpoint).send().await {
                Ok(_) => EmbeddingHealth {
                    available: true,
                    endpoint: self.config.endpoint.clone(),
                    model: self.config.model.clone(),
                    dimensions: self.config.dimensions,
                    latency_ms: Some(start.elapsed().as_millis() as u64),
                    message: "Sidecar reachable".to_string(),
                },
                Err(e) => EmbeddingHealth {
                    available: false,
                    endpoint: self.config.endpoint.clone(),
                    model: self.config.model.clone(),
                    dimensions: self.config.dimensions,
                    latency_ms: None,
                    message: format!("Sidecar unreachable: {e}"),
                },
            }
        };

        match tokio::runtime::Handle::try_current() {
            Ok(handle) => tokio::task::block_in_place(|| handle.block_on(check)),
            Err(_) => {
                if let Ok(rt) = tokio::runtime::Builder::new_current_thread().enable_all().build() {
                    rt.block_on(check)
                } else {
                    EmbeddingHealth {
                        available: false,
                        endpoint: self.config.endpoint.clone(),
                        model: self.config.model.clone(),
                        dimensions: self.config.dimensions,
                        latency_ms: None,
                        message: "Failed to create runtime for health check".to_string(),
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(!provider.is_available());
        let health = provider.health();
        assert!(!health.available);
        assert!(matches!(
            provider.embed_batch(&["hello".to_string()]),
            Err(EmbeddingError::ProviderOffline(_))
        ));
    }

    #[test]
    fn test_sidecar_provider_disabled() {
        let provider = SidecarEmbeddingProvider::default();
        assert_eq!(provider.dimensions(), 768);
        assert!(!provider.is_available());
        let health = provider.health();
        assert!(!health.available);
        assert!(matches!(
            provider.embed_batch(&["hello".to_string()]),
            Err(EmbeddingError::ProviderOffline(_))
        ));
    }
}
