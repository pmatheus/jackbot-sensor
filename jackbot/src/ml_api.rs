use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum MlApiError {
    #[error("HTTP request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("Model not found: {0}")]
    ModelNotFound(String),
    #[error("Invalid state dimension: expected 512, got {0}")]
    InvalidStateDimension(usize),
    #[error("API error: {0}")]
    ApiError(String),
}

/// Request format for model inference
#[derive(Debug, Serialize)]
pub struct StateRequest {
    pub state: Vec<f64>,
    pub model_id: String,
    pub return_activations: bool,
}

/// Response format with predictions and metadata
#[derive(Debug, Deserialize)]
pub struct PredictionResponse {
    pub action: i32,
    pub confidence: f64,
    pub q_values: Vec<f64>,
    pub activations: Option<std::collections::HashMap<String, Vec<f64>>>,
    pub timestamp: String,
    pub model_id: String,
    pub execution_time_ms: f64,
}

/// Model health check response
#[derive(Debug, Deserialize)]
pub struct ModelHealth {
    pub status: String,
    pub models_loaded: Vec<String>,
    pub device: String,
    pub uptime_seconds: f64,
}

/// ML API client for model inference
#[derive(Clone)]
pub struct MlApiClient {
    client: Client,
    base_url: String,
}

impl MlApiClient {
    /// Create a new ML API client
    pub fn new(base_url: impl Into<String>) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            base_url: base_url.into(),
        }
    }

    /// Check API health
    pub async fn health(&self) -> Result<ModelHealth, MlApiError> {
        let url = format!("{}/health", self.base_url);
        let response = self.client.get(&url).send().await?;
        
        if !response.status().is_success() {
            return Err(MlApiError::ApiError(format!(
                "Health check failed: {}",
                response.status()
            )));
        }
        
        Ok(response.json().await?)
    }

    /// Get model prediction for given state
    pub async fn predict(
        &self,
        state: Vec<f64>,
        model_id: &str,
        return_activations: bool,
    ) -> Result<PredictionResponse, MlApiError> {
        if state.len() != 512 {
            return Err(MlApiError::InvalidStateDimension(state.len()));
        }

        let request = StateRequest {
            state,
            model_id: model_id.to_string(),
            return_activations,
        };

        let url = format!("{}/predict", self.base_url);
        let response = self.client.post(&url).json(&request).send().await?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(MlApiError::ApiError(error_text));
        }

        Ok(response.json().await?)
    }

    /// List available models
    pub async fn list_models(&self) -> Result<serde_json::Value, MlApiError> {
        let url = format!("{}/models", self.base_url);
        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            return Err(MlApiError::ApiError(format!(
                "List models failed: {}",
                response.status()
            )));
        }

        Ok(response.json().await?)
    }

    /// Reload models from checkpoints
    pub async fn reload_models(&self) -> Result<serde_json::Value, MlApiError> {
        let url = format!("{}/reload_models", self.base_url);
        let response = self.client.post(&url).send().await?;

        if !response.status().is_success() {
            return Err(MlApiError::ApiError(format!(
                "Reload models failed: {}",
                response.status()
            )));
        }

        Ok(response.json().await?)
    }
}

/// Trait for ML models that can make predictions
#[async_trait]
pub trait AsyncModel: Send + Sync {
    async fn predict(&self, features: &[f64]) -> Result<PredictionResponse, MlApiError>;
}

/// Remote model that calls the ML API
pub struct RemoteModel {
    client: MlApiClient,
    model_id: String,
    return_activations: bool,
}

impl RemoteModel {
    pub fn new(api_url: impl Into<String>, model_id: impl Into<String>) -> Self {
        Self {
            client: MlApiClient::new(api_url),
            model_id: model_id.into(),
            return_activations: false,
        }
    }

    pub fn with_activations(mut self, return_activations: bool) -> Self {
        self.return_activations = return_activations;
        self
    }
}

#[async_trait]
impl AsyncModel for RemoteModel {
    async fn predict(&self, features: &[f64]) -> Result<PredictionResponse, MlApiError> {
        self.client
            .predict(features.to_vec(), &self.model_id, self.return_activations)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_ml_api_client() {
        // This would connect to a real API in production
        let client = MlApiClient::new("http://localhost:8011");
        
        // Test health check
        match client.health().await {
            Ok(health) => {
                println!("API Health: {:?}", health);
                assert!(!health.models_loaded.is_empty());
            }
            Err(e) => {
                println!("Health check failed (expected if API not running): {}", e);
            }
        }
    }
}