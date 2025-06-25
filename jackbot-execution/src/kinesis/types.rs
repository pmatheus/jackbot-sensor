use serde::{Deserialize, Serialize};

/// Stream type enumeration for routing messages
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StreamType {
    OrderExecution,
    StrategyExecution,
    RiskAlerts,
    MarketData,
    SystemEvents,
    UserNotifications,
}

impl StreamType {
    /// Get the stream name prefix for this stream type
    pub fn stream_prefix(&self) -> &'static str {
        match self {
            StreamType::OrderExecution => "jackbot-order-execution",
            StreamType::StrategyExecution => "jackbot-strategy-execution",
            StreamType::RiskAlerts => "jackbot-risk-alerts",
            StreamType::MarketData => "jackbot-market-data",
            StreamType::SystemEvents => "jackbot-system-events",
            StreamType::UserNotifications => "jackbot-user-notifications",
        }
    }
}

impl From<&str> for StreamType {
    fn from(s: &str) -> Self {
        match s {
            "jackbot-order-execution" => StreamType::OrderExecution,
            "jackbot-strategy-execution" => StreamType::StrategyExecution,
            "jackbot-risk-alerts" => StreamType::RiskAlerts,
            "jackbot-market-data" => StreamType::MarketData,
            "jackbot-system-events" => StreamType::SystemEvents,
            "jackbot-user-notifications" => StreamType::UserNotifications,
            _ => StreamType::SystemEvents, // Default fallback
        }
    }
}

/// Consumer group configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsumerGroup {
    pub group_id: String,
    pub region: String,
    pub instance_id: String,
    pub checkpoint_interval_ms: u64,
}

impl Default for ConsumerGroup {
    fn default() -> Self {
        Self {
            group_id: "jackbot-sensor-group".to_string(),
            region: "us-east-1".to_string(),
            instance_id: uuid::Uuid::new_v4().to_string(),
            checkpoint_interval_ms: 30000, // 30 seconds
        }
    }
}

/// Message processing status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ProcessingStatus {
    Pending,
    Processing,
    Completed,
    Failed,
    Retrying,
    DeadLettered,
}

/// Message metadata for tracking and debugging
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageMetadata {
    pub record_id: String,
    pub partition_key: String,
    pub sequence_number: String,
    pub approximate_arrival_timestamp: i64,
    pub processing_attempts: u32,
    pub first_attempt_at: Option<i64>,
    pub last_attempt_at: Option<i64>,
    pub status: ProcessingStatus,
    pub error_details: Option<String>,
}

/// Shard processing state
#[derive(Debug, Clone)]
pub struct ShardState {
    pub shard_id: String,
    pub iterator: Option<String>,
    pub last_sequence_number: Option<String>,
    pub is_processing: bool,
    pub error_count: u32,
    pub last_error: Option<String>,
    pub last_checkpoint: Option<i64>,
}

impl ShardState {
    pub fn new(shard_id: String) -> Self {
        Self {
            shard_id,
            iterator: None,
            last_sequence_number: None,
            is_processing: false,
            error_count: 0,
            last_error: None,
            last_checkpoint: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stream_type_conversion() {
        assert_eq!(
            StreamType::from("jackbot-order-execution"),
            StreamType::OrderExecution
        );
        assert_eq!(
            StreamType::from("jackbot-strategy-execution"),
            StreamType::StrategyExecution
        );
        assert_eq!(
            StreamType::from("jackbot-risk-alerts"),
            StreamType::RiskAlerts
        );
        assert_eq!(StreamType::from("unknown-stream"), StreamType::SystemEvents);
    }

    #[test]
    fn test_stream_type_prefix() {
        assert_eq!(
            StreamType::OrderExecution.stream_prefix(),
            "jackbot-order-execution"
        );
        assert_eq!(
            StreamType::StrategyExecution.stream_prefix(),
            "jackbot-strategy-execution"
        );
        assert_eq!(
            StreamType::RiskAlerts.stream_prefix(),
            "jackbot-risk-alerts"
        );
    }
}
