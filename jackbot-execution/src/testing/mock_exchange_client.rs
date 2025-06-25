// Stub file for mock exchange client module
// This module provides placeholder implementations for mock exchange clients

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MockExchangeClient {
    name: String,
    config: ClientConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientConfig {
    pub simulate_latency: bool,
    pub latency_ms: u64,
}

impl MockExchangeClient {
    pub fn new(name: String) -> Self {
        Self {
            name,
            config: ClientConfig {
                simulate_latency: true,
                latency_ms: 100,
            },
        }
    }

    pub async fn connect(&mut self) -> Result<(), MockError> {
        // Placeholder implementation
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MockError {
    #[error("Mock error: {0}")]
    MockFailed(String),
}
