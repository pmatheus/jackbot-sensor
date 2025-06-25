//! Staking operation error types

use jackbot_instrument::{asset::name::AssetNameExchange, exchange::ExchangeId};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Comprehensive staking error types
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Error)]
pub enum StakingError {
    /// Insufficient balance for staking operation
    #[error("Insufficient balance: required {required}, available {available}")]
    InsufficientBalance {
        required: Decimal,
        available: Decimal,
    },

    /// Staking product is not available
    #[error("Product {product_id} not available on {exchange}")]
    ProductNotAvailable {
        exchange: ExchangeId,
        product_id: String,
    },

    /// Minimum staking amount not met
    #[error("Minimum amount not met: required {minimum}, provided {provided}")]
    MinimumAmountNotMet { minimum: Decimal, provided: Decimal },

    /// Maximum staking amount exceeded
    #[error("Maximum amount exceeded: maximum {maximum}, provided {provided}")]
    MaximumAmountExceeded { maximum: Decimal, provided: Decimal },

    /// Staking quota exceeded
    #[error(
        "Quota exceeded for product {product_id}: available {available}, requested {requested}"
    )]
    QuotaExceeded {
        product_id: String,
        available: Decimal,
        requested: Decimal,
    },

    /// Position is still in lock period
    #[error("Position {position_id} is locked until {unlock_time}")]
    LockPeriodActive {
        position_id: String,
        unlock_time: chrono::DateTime<chrono::Utc>,
    },

    /// Position not found
    #[error("Position {position_id} not found on {exchange}")]
    PositionNotFound {
        exchange: ExchangeId,
        position_id: String,
    },

    /// Asset not supported for staking
    #[error("Asset {asset} not supported for staking on {exchange}")]
    AssetNotSupported {
        exchange: ExchangeId,
        asset: AssetNameExchange,
    },

    /// Network connectivity error
    #[error("Network error: {message}")]
    NetworkError { message: String },

    /// Exchange-specific API error
    #[error("Exchange API error ({exchange}): {code} - {message}")]
    ExchangeError {
        exchange: ExchangeId,
        code: String,
        message: String,
    },

    /// Rate limit exceeded
    #[error("Rate limit exceeded for {exchange}")]
    RateLimitExceeded { exchange: ExchangeId },

    /// Authentication error
    #[error("Authentication failed for {exchange}: {message}")]
    AuthenticationError {
        exchange: ExchangeId,
        message: String,
    },

    /// Invalid staking parameters
    #[error("Invalid parameters: {message}")]
    InvalidParameters { message: String },

    /// Operation already in progress
    #[error("Operation {operation_id} already in progress")]
    OperationInProgress { operation_id: String },

    /// Operation timeout
    #[error("Operation {operation_id} timed out after {timeout_seconds} seconds")]
    OperationTimeout {
        operation_id: String,
        timeout_seconds: u64,
    },

    /// Reward claiming failed
    #[error("Failed to claim rewards for position {position_id}: {reason}")]
    RewardClaimFailed { position_id: String, reason: String },

    /// Auto-compound not supported
    #[error("Auto-compound not supported for product {product_id} on {exchange}")]
    AutoCompoundNotSupported {
        exchange: ExchangeId,
        product_id: String,
    },

    /// Risk limit exceeded
    #[error("Risk limit exceeded: {risk_type} limit is {limit}, attempted {attempted}")]
    RiskLimitExceeded {
        risk_type: String,
        limit: Decimal,
        attempted: Decimal,
    },

    /// Portfolio constraint violation
    #[error("Portfolio constraint violated: {constraint}")]
    ConstraintViolation { constraint: String },

    /// Serialization/deserialization error
    #[error("Serialization error: {message}")]
    SerializationError { message: String },

    /// Configuration error
    #[error("Configuration error: {message}")]
    ConfigurationError { message: String },

    /// Internal system error
    #[error("Internal error: {message}")]
    InternalError { message: String },
}

impl StakingError {
    /// Check if the error is recoverable (can be retried)
    pub fn is_recoverable(&self) -> bool {
        match self {
            Self::NetworkError { .. } => true,
            Self::RateLimitExceeded { .. } => true,
            Self::OperationTimeout { .. } => true,
            Self::ExchangeError { code, .. } => {
                // Some exchange errors are recoverable
                !matches!(
                    code.as_str(),
                    "INVALID_ASSET" | "INVALID_PRODUCT" | "AUTHENTICATION_FAILED"
                )
            }
            _ => false,
        }
    }

    /// Get the severity level of the error
    pub fn severity(&self) -> ErrorSeverity {
        match self {
            Self::InternalError { .. } => ErrorSeverity::Critical,
            Self::AuthenticationError { .. } => ErrorSeverity::Critical,
            Self::ConfigurationError { .. } => ErrorSeverity::High,
            Self::RiskLimitExceeded { .. } => ErrorSeverity::High,
            Self::ConstraintViolation { .. } => ErrorSeverity::High,
            Self::NetworkError { .. } => ErrorSeverity::Medium,
            Self::RateLimitExceeded { .. } => ErrorSeverity::Medium,
            Self::OperationTimeout { .. } => ErrorSeverity::Medium,
            _ => ErrorSeverity::Low,
        }
    }

    /// Get the error category for logging and monitoring
    pub fn category(&self) -> ErrorCategory {
        match self {
            Self::NetworkError { .. } => ErrorCategory::Network,
            Self::RateLimitExceeded { .. } => ErrorCategory::RateLimit,
            Self::AuthenticationError { .. } => ErrorCategory::Authentication,
            Self::ExchangeError { .. } => ErrorCategory::Exchange,
            Self::InsufficientBalance { .. } => ErrorCategory::Balance,
            Self::ProductNotAvailable { .. } => ErrorCategory::Product,
            Self::MinimumAmountNotMet { .. } => ErrorCategory::Validation,
            Self::MaximumAmountExceeded { .. } => ErrorCategory::Validation,
            Self::QuotaExceeded { .. } => ErrorCategory::Quota,
            Self::LockPeriodActive { .. } => ErrorCategory::LockPeriod,
            Self::PositionNotFound { .. } => ErrorCategory::Position,
            Self::AssetNotSupported { .. } => ErrorCategory::Asset,
            Self::InvalidParameters { .. } => ErrorCategory::Validation,
            Self::OperationInProgress { .. } => ErrorCategory::Operation,
            Self::OperationTimeout { .. } => ErrorCategory::Operation,
            Self::RewardClaimFailed { .. } => ErrorCategory::Reward,
            Self::AutoCompoundNotSupported { .. } => ErrorCategory::Feature,
            Self::RiskLimitExceeded { .. } => ErrorCategory::Risk,
            Self::ConstraintViolation { .. } => ErrorCategory::Risk,
            Self::SerializationError { .. } => ErrorCategory::Serialization,
            Self::ConfigurationError { .. } => ErrorCategory::Configuration,
            Self::InternalError { .. } => ErrorCategory::Internal,
        }
    }
}

/// Error severity levels
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorSeverity {
    Low,
    Medium,
    High,
    Critical,
}

/// Error categories for monitoring and alerting
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorCategory {
    Network,
    RateLimit,
    Authentication,
    Exchange,
    Balance,
    Product,
    Validation,
    Quota,
    LockPeriod,
    Position,
    Asset,
    Operation,
    Reward,
    Feature,
    Risk,
    Serialization,
    Configuration,
    Internal,
}

impl From<serde_json::Error> for StakingError {
    fn from(err: serde_json::Error) -> Self {
        Self::SerializationError {
            message: err.to_string(),
        }
    }
}

impl From<reqwest::Error> for StakingError {
    fn from(err: reqwest::Error) -> Self {
        Self::NetworkError {
            message: err.to_string(),
        }
    }
}

/// Result type for staking operations
pub type StakingResult<T> = Result<T, StakingError>;

/// Type alias for recoverable staking operations that can be retried
pub type RecoverableStakingResult<T> = Result<T, (StakingError, bool)>;

impl StakingError {
    /// Convert to recoverable result format
    pub fn to_recoverable(self) -> (Self, bool) {
        let recoverable = self.is_recoverable();
        (self, recoverable)
    }
}
