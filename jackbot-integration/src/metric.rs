//! Metrics collection for monitoring and observability

use std::collections::HashMap;
use serde::{Serialize, Deserialize};

/// Metric field value
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Field {
    String(String),
    Integer(i64),
    Float(f64),
    Boolean(bool),
}

impl From<String> for Field {
    fn from(s: String) -> Self {
        Field::String(s)
    }
}

impl From<&str> for Field {
    fn from(s: &str) -> Self {
        Field::String(s.to_string())
    }
}

impl From<i64> for Field {
    fn from(i: i64) -> Self {
        Field::Integer(i)
    }
}

impl From<u64> for Field {
    fn from(u: u64) -> Self {
        Field::Integer(u as i64)
    }
}

impl From<f64> for Field {
    fn from(f: f64) -> Self {
        Field::Float(f)
    }
}

impl From<bool> for Field {
    fn from(b: bool) -> Self {
        Field::Boolean(b)
    }
}

/// Metric tag for categorization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    pub key: String,
    pub value: String,
}

impl Tag {
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
        }
    }
}

/// A metric measurement
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metric {
    pub name: String,
    pub fields: HashMap<String, Field>,
    pub tags: Vec<Tag>,
    pub timestamp: i64,
}

impl Metric {
    /// Create a new metric
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            fields: HashMap::new(),
            tags: Vec::new(),
            timestamp: chrono::Utc::now().timestamp_millis(),
        }
    }
    
    /// Add a field to the metric
    pub fn field(mut self, key: impl Into<String>, value: impl Into<Field>) -> Self {
        self.fields.insert(key.into(), value.into());
        self
    }
    
    /// Add a tag to the metric
    pub fn tag(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.tags.push(Tag::new(key, value));
        self
    }
    
    /// Set the timestamp
    pub fn timestamp(mut self, timestamp: i64) -> Self {
        self.timestamp = timestamp;
        self
    }
}

/// Metrics collector for aggregating measurements
#[derive(Debug, Default)]
pub struct MetricsCollector {
    metrics: Vec<Metric>,
}

impl MetricsCollector {
    /// Create a new metrics collector
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Record a metric
    pub fn record(&mut self, metric: Metric) {
        self.metrics.push(metric);
    }
    
    /// Get all recorded metrics
    pub fn metrics(&self) -> &[Metric] {
        &self.metrics
    }
    
    /// Clear all recorded metrics
    pub fn clear(&mut self) {
        self.metrics.clear();
    }
}