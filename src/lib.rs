// Infinite Agent Mode - Minimal Library Implementation
// This version provides minimal functionality to support the infinite agent binary

pub mod api;
pub mod config;
pub mod sensor;
pub mod connector;
pub mod streaming;
pub mod validation;
pub mod rate_limit;
pub mod order_processor;
pub mod monitor;
pub mod discovery;
pub mod distribution;

// Minimal placeholder for library functionality
pub struct JackbotSensor;

impl Default for JackbotSensor {
    fn default() -> Self {
        Self::new()
    }
}

impl JackbotSensor {
    pub fn new() -> Self {
        Self
    }
}
