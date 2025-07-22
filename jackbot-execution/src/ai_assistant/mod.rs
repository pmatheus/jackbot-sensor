//! AI Assistant integration for Jackbot Trading Platform
//! 
//! This module provides GPT-4 powered AI assistance for trading operations including:
//! - Real-time market analysis and commentary
//! - Trading strategy suggestions and optimization
//! - Risk assessment and portfolio warnings
//! - Educational guidance and best practices
//! - News analysis and market impact assessment
//! - Cross-asset correlation insights

use crate::error::{SensorOrderError, UnindexedOrderError};
use chrono::{DateTime, Utc};
use reqwest::Client;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, error, info, warn};

// TODO: Implement these AI assistant modules
// pub mod analysis;
// pub mod chat;
// pub mod client;
// pub mod insights;
// pub mod models;
// pub mod strategy;

// pub use analysis::*;
// pub use chat::*;
// pub use client::*;
// pub use insights::*;
// pub use models::*;
// pub use strategy::*;

/// AI Assistant service configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AIAssistantConfig {
    /// OpenAI API key for GPT-4 access
    pub openai_api_key: String,
    /// GPT model version to use
    pub model: String,
    /// Maximum tokens per request
    pub max_tokens: u32,
    /// Temperature for response creativity (0.0-1.0)
    pub temperature: f32,
    /// Request timeout in seconds
    pub timeout_seconds: u64,
    /// Enable streaming responses
    pub enable_streaming: bool,
    /// Cost optimization settings
    pub cost_optimization: CostOptimizationConfig,
    /// Content filtering settings
    pub content_filtering: ContentFilteringConfig,
}

impl Default for AIAssistantConfig {
    fn default() -> Self {
        Self {
            openai_api_key: std::env::var("OPENAI_API_KEY").unwrap_or_default(),
            model: "gpt-4-turbo-preview".to_string(),
            max_tokens: 4096,
            temperature: 0.7,
            timeout_seconds: 30,
            enable_streaming: true,
            cost_optimization: CostOptimizationConfig::default(),
            content_filtering: ContentFilteringConfig::default(),
        }
    }
}

/// Cost optimization configuration for AI API usage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostOptimizationConfig {
    /// Maximum monthly spending limit in USD
    pub monthly_limit_usd: f64,
    /// Daily request limit
    pub daily_request_limit: u32,
    /// Cache responses for similar queries
    pub enable_response_caching: bool,
    /// Cache TTL in seconds
    pub cache_ttl_seconds: u64,
    /// Enable request batching
    pub enable_batching: bool,
}

impl Default for CostOptimizationConfig {
    fn default() -> Self {
        Self {
            monthly_limit_usd: 1000.0,
            daily_request_limit: 1000,
            enable_response_caching: true,
            cache_ttl_seconds: 3600, // 1 hour
            enable_batching: true,
        }
    }
}

/// Content filtering configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentFilteringConfig {
    /// Enable financial advice disclaimer
    pub enable_disclaimer: bool,
    /// Filter out potentially harmful trading advice
    pub filter_harmful_advice: bool,
    /// Enable regulatory compliance checks
    pub regulatory_compliance: bool,
}

impl Default for ContentFilteringConfig {
    fn default() -> Self {
        Self {
            enable_disclaimer: true,
            filter_harmful_advice: true,
            regulatory_compliance: true,
        }
    }
}

/// AI Assistant interaction context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AIContext {
    /// User ID for personalization
    pub user_id: String,
    /// Trading session ID
    pub session_id: String,
    /// Current portfolio state
    pub portfolio: Option<PortfolioSnapshot>,
    /// Recent market data
    pub market_data: Option<MarketDataSnapshot>,
    /// Conversation history
    pub conversation_history: Vec<ConversationMessage>,
    /// User preferences and risk profile
    pub user_profile: Option<UserProfile>,
    /// Current market conditions
    pub market_conditions: Option<MarketConditions>,
}

/// Portfolio snapshot for AI analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioSnapshot {
    /// Total portfolio value in USD
    pub total_value_usd: Decimal,
    /// Asset allocations
    pub allocations: HashMap<String, AssetAllocation>,
    /// Open positions
    pub open_positions: Vec<Position>,
    /// Recent trades
    pub recent_trades: Vec<TradeSnapshot>,
    /// Performance metrics
    pub performance: PerformanceMetrics,
    /// Risk metrics
    pub risk_metrics: RiskMetrics,
}

/// Asset allocation in portfolio
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetAllocation {
    /// Asset symbol
    pub symbol: String,
    /// Quantity held
    pub quantity: Decimal,
    /// Current value in USD
    pub value_usd: Decimal,
    /// Percentage of total portfolio
    pub percentage: f64,
    /// Average cost basis
    pub cost_basis: Decimal,
    /// Unrealized PnL
    pub unrealized_pnl: Decimal,
}

/// Open position details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    /// Position ID
    pub id: String,
    /// Asset symbol
    pub symbol: String,
    /// Position size
    pub size: Decimal,
    /// Side (long/short)
    pub side: String,
    /// Entry price
    pub entry_price: Decimal,
    /// Current price
    pub current_price: Decimal,
    /// Unrealized PnL
    pub unrealized_pnl: Decimal,
    /// Position age
    pub age_hours: f64,
    /// Stop loss level
    pub stop_loss: Option<Decimal>,
    /// Take profit level
    pub take_profit: Option<Decimal>,
}

/// Trade snapshot for analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeSnapshot {
    /// Trade ID
    pub id: String,
    /// Asset symbol
    pub symbol: String,
    /// Trade side
    pub side: String,
    /// Quantity traded
    pub quantity: Decimal,
    /// Execution price
    pub price: Decimal,
    /// Trade timestamp
    pub timestamp: DateTime<Utc>,
    /// Realized PnL
    pub realized_pnl: Option<Decimal>,
    /// Trade strategy
    pub strategy: Option<String>,
}

/// Performance metrics for portfolio
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    /// Total return percentage
    pub total_return_pct: f64,
    /// Annualized return
    pub annualized_return_pct: f64,
    /// Sharpe ratio
    pub sharpe_ratio: f64,
    /// Maximum drawdown
    pub max_drawdown_pct: f64,
    /// Win rate for trades
    pub win_rate_pct: f64,
    /// Average trade return
    pub avg_trade_return_pct: f64,
    /// Volatility
    pub volatility_pct: f64,
}

/// Risk metrics for portfolio analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskMetrics {
    /// Value at Risk (95% confidence)
    pub var_95_usd: Decimal,
    /// Expected Shortfall
    pub expected_shortfall_usd: Decimal,
    /// Portfolio concentration risk
    pub concentration_risk: f64,
    /// Leverage ratio
    pub leverage_ratio: f64,
    /// Correlation with major indices
    pub market_correlation: f64,
    /// Liquidity risk score
    pub liquidity_risk: f64,
}

/// Market data snapshot for AI context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketDataSnapshot {
    /// Timestamp of snapshot
    pub timestamp: DateTime<Utc>,
    /// Asset prices
    pub prices: HashMap<String, Decimal>,
    /// 24h price changes
    pub price_changes_24h: HashMap<String, f64>,
    /// Volume data
    pub volumes_24h: HashMap<String, Decimal>,
    /// Market cap data
    pub market_caps: HashMap<String, Decimal>,
    /// Fear & Greed index
    pub fear_greed_index: Option<u8>,
    /// Volatility index
    pub volatility_index: Option<f64>,
}

/// User profile for personalized AI responses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfile {
    /// Risk tolerance (1-10)
    pub risk_tolerance: u8,
    /// Trading experience level
    pub experience_level: ExperienceLevel,
    /// Preferred trading style
    pub trading_style: TradingStyle,
    /// Investment goals
    pub investment_goals: Vec<InvestmentGoal>,
    /// Time horizon
    pub time_horizon: TimeHorizon,
    /// Preferred assets
    pub preferred_assets: Vec<String>,
    /// Language preference
    pub language: String,
}

/// Trading experience levels
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExperienceLevel {
    Beginner,
    Intermediate,
    Advanced,
    Professional,
}

/// Trading style preferences
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TradingStyle {
    Scalping,
    DayTrading,
    SwingTrading,
    PositionTrading,
    Arbitrage,
    MarketMaking,
}

/// Investment goals
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InvestmentGoal {
    CapitalPreservation,
    IncomeGeneration,
    CapitalAppreciation,
    Speculation,
    Hedging,
    Diversification,
}

/// Time horizon for investments
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TimeHorizon {
    ShortTerm,   // < 1 year
    MediumTerm,  // 1-5 years
    LongTerm,    // > 5 years
}

/// Market conditions for context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketConditions {
    /// Overall market trend
    pub trend: MarketTrend,
    /// Volatility level
    pub volatility: VolatilityLevel,
    /// Liquidity conditions
    pub liquidity: LiquidityLevel,
    /// Market sentiment
    pub sentiment: MarketSentiment,
    /// Economic indicators
    pub economic_indicators: HashMap<String, f64>,
    /// Recent news events
    pub recent_news: Vec<NewsEvent>,
}

/// Market trend classifications
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MarketTrend {
    StrongUptrend,
    Uptrend,
    Sideways,
    Downtrend,
    StrongDowntrend,
}

/// Volatility level classifications
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VolatilityLevel {
    VeryLow,
    Low,
    Normal,
    High,
    VeryHigh,
}

/// Liquidity level classifications
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LiquidityLevel {
    VeryTight,
    Tight,
    Normal,
    Good,
    Excellent,
}

/// Market sentiment classifications
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MarketSentiment {
    ExtremeFear,
    Fear,
    Neutral,
    Greed,
    ExtremeGreed,
}

/// News event for market context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewsEvent {
    /// Event title
    pub title: String,
    /// Event summary
    pub summary: String,
    /// Event timestamp
    pub timestamp: DateTime<Utc>,
    /// Impact score (-1.0 to 1.0)
    pub impact_score: f64,
    /// Affected assets
    pub affected_assets: Vec<String>,
    /// News source
    pub source: String,
    /// Sentiment score
    pub sentiment_score: f64,
}

/// Conversation message for chat history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationMessage {
    /// Message ID
    pub id: String,
    /// Message role (user/assistant/system)
    pub role: MessageRole,
    /// Message content
    pub content: String,
    /// Message timestamp
    pub timestamp: DateTime<Utc>,
    /// Message metadata
    pub metadata: Option<HashMap<String, String>>,
}

/// Message roles in conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

/// AI Assistant response types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AIResponse {
    /// Market analysis response
    MarketAnalysis(MarketAnalysisResponse),
    /// Trading strategy suggestion
    StrategyAdvice(StrategyAdviceResponse),
    /// Risk assessment
    RiskAssessment(RiskAssessmentResponse),
    /// Educational content
    Educational(EducationalResponse),
    /// News analysis
    NewsAnalysis(NewsAnalysisResponse),
    /// Portfolio review
    PortfolioReview(PortfolioReviewResponse),
    /// General chat response
    Chat(ChatResponse),
    /// Error response
    Error(ErrorResponse),
}

/// Market analysis response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketAnalysisResponse {
    /// Analysis summary
    pub summary: String,
    /// Key insights
    pub insights: Vec<String>,
    /// Price predictions
    pub predictions: Vec<PricePrediction>,
    /// Risk factors
    pub risk_factors: Vec<String>,
    /// Opportunities
    pub opportunities: Vec<String>,
    /// Confidence score
    pub confidence: f64,
    /// Analysis timestamp
    pub timestamp: DateTime<Utc>,
}

/// Price prediction data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PricePrediction {
    /// Asset symbol
    pub symbol: String,
    /// Current price
    pub current_price: Decimal,
    /// Predicted price ranges
    pub predictions: Vec<PriceRange>,
    /// Prediction confidence
    pub confidence: f64,
    /// Time horizon
    pub time_horizon: String,
}

/// Price range prediction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceRange {
    /// Time period
    pub period: String,
    /// Low price estimate
    pub low: Decimal,
    /// High price estimate
    pub high: Decimal,
    /// Target price
    pub target: Decimal,
    /// Probability
    pub probability: f64,
}

/// Strategy advice response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyAdviceResponse {
    /// Strategy recommendation
    pub recommendation: String,
    /// Suggested actions
    pub actions: Vec<TradingAction>,
    /// Expected outcomes
    pub expected_outcomes: Vec<String>,
    /// Risk considerations
    pub risks: Vec<String>,
    /// Implementation steps
    pub implementation_steps: Vec<String>,
    /// Strategy confidence
    pub confidence: f64,
}

/// Trading action suggestion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingAction {
    /// Action type
    pub action_type: ActionType,
    /// Asset symbol
    pub symbol: String,
    /// Suggested quantity
    pub quantity: Option<Decimal>,
    /// Suggested price
    pub price: Option<Decimal>,
    /// Reasoning
    pub reasoning: String,
    /// Priority level
    pub priority: Priority,
    /// Time sensitivity
    pub time_sensitivity: TimeSensitivity,
}

/// Action types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ActionType {
    Buy,
    Sell,
    Hold,
    ReducePosition,
    IncreasePosition,
    Hedge,
    SetStopLoss,
    SetTakeProfit,
    Rebalance,
}

/// Priority levels
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Priority {
    Low,
    Medium,
    High,
    Critical,
}

/// Time sensitivity levels
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TimeSensitivity {
    NoRush,
    SoonPreferred,
    Urgent,
    Immediate,
}

/// Risk assessment response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskAssessmentResponse {
    /// Overall risk score (1-10)
    pub overall_risk_score: u8,
    /// Risk breakdown
    pub risk_breakdown: HashMap<String, f64>,
    /// Warning messages
    pub warnings: Vec<RiskWarning>,
    /// Mitigation suggestions
    pub mitigation_suggestions: Vec<String>,
    /// Portfolio health metrics
    pub health_metrics: HashMap<String, f64>,
}

/// Risk warning message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskWarning {
    /// Warning type
    pub warning_type: RiskWarningType,
    /// Warning message
    pub message: String,
    /// Severity level
    pub severity: Severity,
    /// Affected assets
    pub affected_assets: Vec<String>,
    /// Suggested actions
    pub suggested_actions: Vec<String>,
}

/// Risk warning types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RiskWarningType {
    Concentration,
    Leverage,
    Volatility,
    Liquidity,
    Correlation,
    Drawdown,
    VaR,
}

/// Severity levels
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Severity {
    Info,
    Warning,
    High,
    Critical,
}

/// Educational response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EducationalResponse {
    /// Educational content
    pub content: String,
    /// Learning objectives
    pub learning_objectives: Vec<String>,
    /// Key concepts
    pub key_concepts: Vec<KeyConcept>,
    /// Further reading suggestions
    pub further_reading: Vec<String>,
    /// Interactive examples
    pub examples: Vec<TradingExample>,
}

/// Key concept explanation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyConcept {
    /// Concept name
    pub name: String,
    /// Definition
    pub definition: String,
    /// Practical application
    pub application: String,
    /// Related concepts
    pub related_concepts: Vec<String>,
}

/// Trading example for education
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingExample {
    /// Example title
    pub title: String,
    /// Scenario description
    pub scenario: String,
    /// Analysis steps
    pub analysis_steps: Vec<String>,
    /// Outcome explanation
    pub outcome: String,
    /// Lessons learned
    pub lessons: Vec<String>,
}

/// News analysis response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewsAnalysisResponse {
    /// News summary
    pub summary: String,
    /// Market impact assessment
    pub market_impact: MarketImpactAssessment,
    /// Affected assets analysis
    pub affected_assets: Vec<AssetImpactAnalysis>,
    /// Trading implications
    pub trading_implications: Vec<String>,
    /// Timeline expectations
    pub timeline: Vec<TimelineEvent>,
}

/// Market impact assessment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketImpactAssessment {
    /// Overall impact score (-1.0 to 1.0)
    pub impact_score: f64,
    /// Impact duration
    pub duration: ImpactDuration,
    /// Confidence in assessment
    pub confidence: f64,
    /// Key factors
    pub key_factors: Vec<String>,
}

/// Impact duration classifications
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ImpactDuration {
    Immediate,  // Minutes to hours
    ShortTerm,  // Hours to days
    MediumTerm, // Days to weeks
    LongTerm,   // Weeks to months
}

/// Asset-specific impact analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetImpactAnalysis {
    /// Asset symbol
    pub symbol: String,
    /// Impact score for this asset
    pub impact_score: f64,
    /// Price target adjustment
    pub price_target_adjustment: Option<f64>,
    /// Volatility impact
    pub volatility_impact: f64,
    /// Trading volume impact
    pub volume_impact: f64,
    /// Key impact factors
    pub impact_factors: Vec<String>,
}

/// Timeline event for news impact
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineEvent {
    /// Event description
    pub description: String,
    /// Expected timeframe
    pub timeframe: String,
    /// Probability
    pub probability: f64,
    /// Market impact
    pub market_impact: f64,
}

/// Portfolio review response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioReviewResponse {
    /// Overall assessment
    pub overall_assessment: String,
    /// Strengths identified
    pub strengths: Vec<String>,
    /// Areas for improvement
    pub improvement_areas: Vec<String>,
    /// Rebalancing suggestions
    pub rebalancing_suggestions: Vec<RebalancingSuggestion>,
    /// Performance analysis
    pub performance_analysis: PerformanceAnalysis,
    /// Risk analysis
    pub risk_analysis: RiskAnalysis,
}

/// Rebalancing suggestion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RebalancingSuggestion {
    /// Asset symbol
    pub symbol: String,
    /// Current allocation percentage
    pub current_allocation: f64,
    /// Suggested allocation percentage
    pub suggested_allocation: f64,
    /// Reasoning for change
    pub reasoning: String,
    /// Expected benefits
    pub expected_benefits: Vec<String>,
}

/// Performance analysis details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceAnalysis {
    /// Performance summary
    pub summary: String,
    /// Benchmark comparison
    pub benchmark_comparison: Vec<BenchmarkComparison>,
    /// Attribution analysis
    pub attribution: Vec<AttributionFactor>,
    /// Improvement suggestions
    pub improvement_suggestions: Vec<String>,
}

/// Benchmark comparison data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkComparison {
    /// Benchmark name
    pub benchmark: String,
    /// Portfolio return
    pub portfolio_return: f64,
    /// Benchmark return
    pub benchmark_return: f64,
    /// Outperformance
    pub outperformance: f64,
    /// Risk-adjusted performance
    pub risk_adjusted_performance: f64,
}

/// Attribution factor for performance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttributionFactor {
    /// Factor name
    pub factor: String,
    /// Contribution to performance
    pub contribution: f64,
    /// Explanation
    pub explanation: String,
}

/// Risk analysis details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskAnalysis {
    /// Risk summary
    pub summary: String,
    /// Risk factors
    pub risk_factors: Vec<RiskFactor>,
    /// Stress test results
    pub stress_tests: Vec<StressTestResult>,
    /// Risk mitigation suggestions
    pub mitigation_suggestions: Vec<String>,
}

/// Risk factor analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskFactor {
    /// Factor name
    pub name: String,
    /// Risk level (1-10)
    pub risk_level: u8,
    /// Impact description
    pub impact: String,
    /// Mitigation strategies
    pub mitigation_strategies: Vec<String>,
}

/// Stress test result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StressTestResult {
    /// Scenario name
    pub scenario: String,
    /// Portfolio impact
    pub portfolio_impact: f64,
    /// Affected positions
    pub affected_positions: Vec<String>,
    /// Recovery timeframe
    pub recovery_timeframe: String,
}

/// Chat response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    /// Response content
    pub content: String,
    /// Follow-up suggestions
    pub follow_up_suggestions: Vec<String>,
    /// Related topics
    pub related_topics: Vec<String>,
    /// Confidence in response
    pub confidence: f64,
}

/// Error response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    /// Error message
    pub message: String,
    /// Error code
    pub code: String,
    /// Recovery suggestions
    pub recovery_suggestions: Vec<String>,
}

/// AI Assistant service errors
#[derive(Debug, thiserror::Error)]
pub enum AIAssistantError {
    #[error("OpenAI API error: {0}")]
    OpenAI(String),
    
    #[error("Request timeout after {0}s")]
    Timeout(u64),
    
    #[error("Cost limit exceeded: {current}$ > {limit}$")]
    CostLimitExceeded { current: f64, limit: f64 },
    
    #[error("Rate limit exceeded: {requests} requests > {limit} limit")]
    RateLimitExceeded { requests: u32, limit: u32 },
    
    #[error("Invalid API key")]
    InvalidApiKey,
    
    #[error("Content filtering violation: {reason}")]
    ContentFiltering { reason: String },
    
    #[error("Configuration error: {0}")]
    Configuration(String),
    
    #[error("Context too large: {tokens} tokens > {limit} limit")]
    ContextTooLarge { tokens: u32, limit: u32 },
    
    #[error("Unsupported operation: {operation}")]
    UnsupportedOperation { operation: String },
    
    #[error("Data processing error: {0}")]
    DataProcessing(String),
    
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),
    
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

impl From<AIAssistantError> for SensorOrderError {
    fn from(error: AIAssistantError) -> Self {
        Self::SensorExecutionError {
            context: format!("AI Assistant error: {}", error),
        }
    }
}

impl From<AIAssistantError> for UnindexedOrderError {
    fn from(error: AIAssistantError) -> Self {
        Self::Connectivity(crate::error::ConnectivityError::Socket(format!(
            "AI Assistant error: {}",
            error
        )))
    }
}