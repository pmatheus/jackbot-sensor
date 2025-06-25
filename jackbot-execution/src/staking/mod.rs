//! # Staking Operations
//!
//! This module implements comprehensive staking operations across all supported exchanges.
//! It provides a unified interface for staking assets, managing positions, and optimizing yield.
//!
//! ## Core Features
//!
//! - **Flexible Staking**: Instant unstaking with competitive yields
//! - **Locked Staking**: Fixed-term staking with higher yields
//! - **Yield Optimization**: Automated selection of best APY products
//! - **Risk Management**: Diversification and exposure controls
//! - **Real-time Monitoring**: Position tracking and reward accumulation
//!
//! ## Exchange Support
//!
//! - Binance (Simple Earn, DeFi, Launchpool)
//! - Bybit (Savings, Fixed Deposits, Liquid Staking)
//! - OKX (Savings, DeFi Earn, Structured Products)
//! - Kraken (ETH 2.0, DOT, KSM staking)
//! - Coinbase (ETH 2.0, ATOM, XTZ staking)
//! - KuCoin (Pool-X, Soft Staking)
//! - Gate.io (Hodl & Earn, Startup projects)
//! - Bitget (Earn, Launchpad, BGB vault)
//! - MEXC (Savings, Kickstarter, MX zone)
//! - Crypto.com (Earn, Supercharger, CRO staking)

use chrono::{DateTime, Duration, Utc};
use jackbot_instrument::exchange::ExchangeId;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub mod error;
pub mod manager;
pub mod optimizer;
pub mod strategies;
// Additional modules to be implemented
// pub mod risk;
// pub mod portfolio;
// pub mod monitor;
// pub mod analytics;

// Exchange-specific implementations
pub mod binance;
pub mod bybit;
pub mod okx;
// Additional exchange implementations to be added
// pub mod kraken;
// pub mod coinbase;
// pub mod kucoin;
// pub mod gateio;
// pub mod bitget;
// pub mod mexc;
// pub mod crypto_com;

pub use error::StakingError;
pub use manager::StakingManager;

/// Unique identifier for a staking operation
pub type StakeId = String;

/// Unique identifier for an unstaking operation
pub type UnstakeId = String;

/// Unique identifier for a reward claim operation
pub type ClaimId = String;

/// Types of staking products available
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StakingType {
    /// Flexible staking with instant unstaking capability
    Flexible,
    /// Fixed-term staking with locked period
    Locked(Duration),
    /// DeFi staking products
    DeFi,
    /// Liquid staking derivatives
    Liquid,
}

/// Staking product information
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StakingProduct {
    /// Unique product identifier
    pub id: String,
    /// Asset being staked
    pub asset: String,
    /// Exchange offering the product
    pub exchange: ExchangeId,
    /// Type of staking product
    pub product_type: StakingType,
    /// Annual percentage yield
    pub apy: Decimal,
    /// Minimum staking amount
    pub minimum_amount: Decimal,
    /// Maximum staking amount (if applicable)
    pub maximum_amount: Option<Decimal>,
    /// Lock period for locked staking
    pub lock_period: Option<Duration>,
    /// Whether rewards are auto-compounded
    pub auto_compound: bool,
    /// Available quota for staking
    pub available_quota: Option<Decimal>,
    /// Product status
    pub status: StakingProductStatus,
    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

/// Status of a staking product
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StakingProductStatus {
    /// Product is available for staking
    Available,
    /// Product is temporarily unavailable
    Unavailable,
    /// Product is sold out (quota exceeded)
    SoldOut,
    /// Product is deprecated
    Deprecated,
}

/// Current staking position
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StakingPosition {
    /// Unique position identifier
    pub id: String,
    /// Asset being staked
    pub asset: String,
    /// Exchange holding the position
    pub exchange: ExchangeId,
    /// Amount staked
    pub amount: Decimal,
    /// Associated staking product
    pub product: StakingProduct,
    /// When staking started
    pub start_time: DateTime<Utc>,
    /// When staking ends (for locked products)
    pub end_time: Option<DateTime<Utc>>,
    /// Accumulated rewards
    pub accumulated_rewards: Decimal,
    /// Current position status
    pub status: StakingPositionStatus,
    /// Last update timestamp
    pub last_updated: DateTime<Utc>,
}

/// Status of a staking position
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StakingPositionStatus {
    /// Position is active and earning rewards
    Active,
    /// Position is being unstaked
    Unstaking,
    /// Position has been fully unstaked
    Completed,
    /// Position was cancelled
    Cancelled,
    /// Position is expired but not yet unstaked
    Expired,
}

/// Staking rewards information
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StakingReward {
    /// Unique reward identifier
    pub id: String,
    /// Asset of the reward
    pub asset: String,
    /// Exchange providing the reward
    pub exchange: ExchangeId,
    /// Associated staking position
    pub position_id: String,
    /// Reward amount
    pub amount: Decimal,
    /// When the reward was earned
    pub earned_time: DateTime<Utc>,
    /// When the reward was claimed (if applicable)
    pub claimed_time: Option<DateTime<Utc>>,
    /// Reward status
    pub status: StakingRewardStatus,
}

/// Status of staking rewards
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StakingRewardStatus {
    /// Reward is pending distribution
    Pending,
    /// Reward is available for claiming
    Available,
    /// Reward has been claimed
    Claimed,
    /// Reward was auto-compounded
    Compounded,
}

/// Staking operation result
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StakingOperation {
    /// Operation identifier
    pub id: String,
    /// Type of operation
    pub operation_type: StakingOperationType,
    /// Exchange where operation was performed
    pub exchange: ExchangeId,
    /// Asset involved in operation
    pub asset: String,
    /// Amount involved
    pub amount: Decimal,
    /// Operation timestamp
    pub timestamp: DateTime<Utc>,
    /// Operation status
    pub status: StakingOperationStatus,
    /// Error message if operation failed
    pub error: Option<String>,
}

/// Types of staking operations
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StakingOperationType {
    /// Stake operation
    Stake,
    /// Unstake operation
    Unstake,
    /// Claim rewards operation
    ClaimRewards,
    /// Auto-compound operation
    AutoCompound,
}

/// Status of staking operations
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StakingOperationStatus {
    /// Operation is pending
    Pending,
    /// Operation is in progress
    InProgress,
    /// Operation completed successfully
    Success,
    /// Operation failed
    Failed,
    /// Operation was cancelled
    Cancelled,
}

/// Constraints for staking operations
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StakingConstraints {
    /// Minimum APY required
    pub min_apy: Option<Decimal>,
    /// Maximum lock period acceptable
    pub max_lock_period: Option<Duration>,
    /// Preferred staking types
    pub preferred_types: Vec<StakingType>,
    /// Exchanges to include/exclude
    pub exchange_filter: ExchangeFilter,
    /// Risk tolerance level
    pub risk_tolerance: RiskTolerance,
}

/// Exchange filtering options
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExchangeFilter {
    /// Include all exchanges
    All,
    /// Include only specified exchanges
    Include(Vec<ExchangeId>),
    /// Exclude specified exchanges
    Exclude(Vec<ExchangeId>),
}

/// Risk tolerance levels
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RiskTolerance {
    /// Conservative - prefer established products
    Conservative,
    /// Moderate - balanced approach
    Moderate,
    /// Aggressive - maximize yield
    Aggressive,
}

/// Staking recommendation from optimization engine
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StakingRecommendation {
    /// Recommended product
    pub product: StakingProduct,
    /// Recommended amount to stake
    pub amount: Decimal,
    /// Expected annual return
    pub expected_return: Decimal,
    /// Risk score (0-100)
    pub risk_score: u8,
    /// Confidence level (0-100)
    pub confidence: u8,
    /// Reasoning for recommendation
    pub reasoning: String,
}

/// Portfolio allocation strategy
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AllocationStrategy {
    /// Target allocations by exchange
    pub exchange_allocations: HashMap<ExchangeId, Decimal>,
    /// Target allocations by asset
    pub asset_allocations: HashMap<String, Decimal>,
    /// Target allocations by staking type
    pub type_allocations: HashMap<StakingType, Decimal>,
    /// Rebalancing frequency
    pub rebalance_frequency: Duration,
}

/// Rebalancing action
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RebalanceAction {
    /// Action type
    pub action: RebalanceActionType,
    /// Position to rebalance
    pub position_id: String,
    /// Amount to move
    pub amount: Decimal,
    /// Target product (for moves)
    pub target_product: Option<StakingProduct>,
    /// Priority (higher is more urgent)
    pub priority: u8,
}

/// Types of rebalancing actions
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RebalanceActionType {
    /// Unstake from current position
    Unstake,
    /// Move to different product
    Move,
    /// Add more to existing position
    Add,
    /// No action needed
    Hold,
}
