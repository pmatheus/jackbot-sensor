//! Comprehensive tests for staking operations

use chrono::{Duration, Utc};
use jackbot_execution::staking::{
    binance::{BinanceStakingConfig, BinanceStakingManager},
    bybit::{BybitStakingConfig, BybitStakingManager},
    error::StakingError,
    manager::{StakingManager, StakingManagerImpl, UnifiedStakingManager},
    optimizer::{OptimizationParams, RiskSettings, YieldOptimizer},
    strategies::{
        ConservativeStrategy, DiversifiedStrategy, LiquidityConditions, LiquidityFirstStrategy,
        MarketContext, MarketTrend, MaxYieldStrategy, StakingContext, StakingStrategy,
    },
    *,
};
use jackbot_instrument::{asset::name::AssetNameExchange, exchange::ExchangeId};
use rust_decimal::{prelude::FromPrimitive, Decimal};
use std::collections::HashMap;
use tokio;

/// Create mock staking product for testing
fn create_mock_product(
    id: &str,
    asset: &str,
    exchange: ExchangeId,
    apy: f64,
    product_type: StakingType,
) -> StakingProduct {
    StakingProduct {
        id: id.to_string(),
        asset: asset.to_string(),
        exchange,
        product_type: product_type.clone(),
        apy: Decimal::from_f64(apy / 100.0).unwrap(), // Convert percentage to decimal
        minimum_amount: Decimal::from(10),
        maximum_amount: Some(Decimal::from(1000000)),
        lock_period: match &product_type {
            StakingType::Locked(duration) => Some(*duration),
            _ => None,
        },
        auto_compound: false,
        available_quota: Some(Decimal::from(100000)),
        status: StakingProductStatus::Available,
        metadata: HashMap::new(),
    }
}

/// Create mock staking position for testing
fn create_mock_position(
    id: &str,
    asset: &str,
    exchange: ExchangeId,
    amount: f64,
    apy: f64,
) -> StakingPosition {
    let product = create_mock_product("product_1", asset, exchange, apy, StakingType::Flexible);

    StakingPosition {
        id: id.to_string(),
        asset: asset.to_string(),
        exchange,
        amount: Decimal::from_f64(amount).unwrap(),
        product,
        start_time: Utc::now() - Duration::days(30),
        end_time: None,
        accumulated_rewards: Decimal::from_f64(amount * apy / 100.0 / 12.0).unwrap(), // Monthly reward
        status: StakingPositionStatus::Active,
        last_updated: Utc::now(),
    }
}

#[tokio::test]
async fn test_staking_manager_trait() {
    let config = BinanceStakingConfig::default();
    let manager = BinanceStakingManager::new(config);

    // Test exchange ID
    assert_eq!(manager.exchange_id(), ExchangeId::BinanceSpot);

    // Test getting products (should return error due to missing credentials)
    let asset = AssetNameExchange::from("USDT");
    let result = manager.get_staking_products(&asset).await;
    assert!(result.is_err());
}

#[test]
fn test_staking_product_creation() {
    let product = create_mock_product(
        "test_product",
        "USDT",
        ExchangeId::BinanceSpot,
        5.0,
        StakingType::Flexible,
    );

    assert_eq!(product.id, "test_product");
    assert_eq!(product.asset, "USDT");
    assert_eq!(product.exchange, ExchangeId::BinanceSpot);
    assert_eq!(product.apy, Decimal::from_f64(0.05).unwrap());
    assert_eq!(product.product_type, StakingType::Flexible);
    assert_eq!(product.status, StakingProductStatus::Available);
}

#[test]
fn test_staking_position_creation() {
    let position = create_mock_position(
        "test_position",
        "USDT",
        ExchangeId::BinanceSpot,
        1000.0,
        5.0,
    );

    assert_eq!(position.id, "test_position");
    assert_eq!(position.asset, "USDT");
    assert_eq!(position.exchange, ExchangeId::BinanceSpot);
    assert_eq!(position.amount, Decimal::from(1000));
    assert_eq!(position.status, StakingPositionStatus::Active);
}

#[test]
fn test_yield_optimizer() {
    let optimizer = YieldOptimizer::new();

    // Create test products
    let products = vec![
        create_mock_product(
            "product_1",
            "USDT",
            ExchangeId::BinanceSpot,
            5.0,
            StakingType::Flexible,
        ),
        create_mock_product(
            "product_2",
            "USDT",
            ExchangeId::BybitSpot,
            6.0,
            StakingType::Flexible,
        ),
        create_mock_product(
            "product_3",
            "USDT",
            ExchangeId::Okx,
            4.5,
            StakingType::Locked(Duration::days(90)),
        ),
    ];

    let total_amount = Decimal::from(10000);
    let constraints = Some(StakingConstraints {
        min_apy: Some(Decimal::from_f64(0.04).unwrap()), // 4%
        max_lock_period: Some(Duration::days(180)),
        preferred_types: vec![StakingType::Flexible],
        exchange_filter: ExchangeFilter::All,
        risk_tolerance: RiskTolerance::Moderate,
    });

    let result = optimizer.find_best_products(&products, total_amount, constraints.as_ref());
    assert!(result.is_ok());

    let recommendations = result.unwrap();
    assert!(!recommendations.is_empty());

    // Check that recommendations respect constraints
    for rec in &recommendations {
        assert!(rec.product.apy >= Decimal::from_f64(0.04).unwrap());
        assert!(rec.amount > Decimal::ZERO);
        assert!(rec.confidence > 0);
    }
}

#[test]
fn test_yield_optimizer_constraints() {
    // Test optimizer without using unimplemented risk manager
    let optimizer = YieldOptimizer::new();

    // Create test products
    let products = vec![
        create_mock_product(
            "product_1",
            "USDT",
            ExchangeId::BinanceSpot,
            5.0,
            StakingType::Flexible,
        ),
        create_mock_product(
            "product_2",
            "USDT",
            ExchangeId::BybitSpot,
            6.0,
            StakingType::Flexible,
        ),
        create_mock_product(
            "product_3",
            "USDT",
            ExchangeId::Okx,
            4.5,
            StakingType::Locked(Duration::days(90)),
        ),
    ];

    let total_amount = Decimal::from(10000);
    let constraints = Some(StakingConstraints {
        min_apy: Some(Decimal::from_f64(0.04).unwrap()), // 4%
        max_lock_period: Some(Duration::days(180)),
        preferred_types: vec![StakingType::Flexible],
        exchange_filter: ExchangeFilter::All,
        risk_tolerance: RiskTolerance::Moderate,
    });

    let result = optimizer.find_best_products(&products, total_amount, constraints.as_ref());
    assert!(result.is_ok());

    let recommendations = result.unwrap();
    assert!(!recommendations.is_empty());

    // Check that recommendations respect constraints
    for rec in &recommendations {
        assert!(rec.product.apy >= Decimal::from_f64(0.04).unwrap());
        assert!(rec.amount > Decimal::ZERO);
        assert!(rec.confidence > 0);
    }
}

#[tokio::test]
async fn test_max_yield_strategy() {
    let strategy = MaxYieldStrategy::new();

    // Create test context
    let products = vec![
        create_mock_product(
            "product_1",
            "USDT",
            ExchangeId::BinanceSpot,
            5.0,
            StakingType::Flexible,
        ),
        create_mock_product(
            "product_2",
            "USDT",
            ExchangeId::BybitSpot,
            7.0,
            StakingType::Flexible,
        ),
        create_mock_product(
            "product_3",
            "BTC",
            ExchangeId::Okx,
            3.0,
            StakingType::Flexible,
        ),
    ];

    let mut available_balances = HashMap::new();
    available_balances.insert("USDT".to_string(), Decimal::from(10000));
    available_balances.insert("BTC".to_string(), Decimal::from(1));

    let context = StakingContext {
        available_products: products,
        current_positions: vec![],
        available_balances,
        market_context: MarketContext {
            volatility: 30.0,
            trend: MarketTrend::Bullish,
            apy_trends: HashMap::new(),
            liquidity_conditions: LiquidityConditions {
                liquidity_score: 85.0,
                exchange_conditions: HashMap::new(),
            },
        },
        constraints: None,
    };

    let result = strategy.execute(&context).await;
    assert!(result.is_ok());

    let actions = result.unwrap();
    assert!(!actions.is_empty());

    // Verify strategy name and risk assessment
    assert_eq!(strategy.name(), "MaxYield");
    let risk_profile = strategy.risk_assessment();
    assert!(risk_profile.risk_level > 50); // Aggressive strategy
}

#[tokio::test]
async fn test_conservative_strategy() {
    let strategy = ConservativeStrategy::new();

    // Create test context with lower balances (conservative strategy has higher minimums)
    let products = vec![
        create_mock_product(
            "product_1",
            "USDT",
            ExchangeId::BinanceSpot,
            3.0,
            StakingType::Flexible,
        ),
        create_mock_product(
            "product_2",
            "USDT",
            ExchangeId::Coinbase,
            2.5,
            StakingType::Flexible,
        ),
    ];

    let mut available_balances = HashMap::new();
    available_balances.insert("USDT".to_string(), Decimal::from(1000)); // Above $100 minimum

    let context = StakingContext {
        available_products: products,
        current_positions: vec![],
        available_balances,
        market_context: MarketContext {
            volatility: 15.0,
            trend: MarketTrend::Sideways,
            apy_trends: HashMap::new(),
            liquidity_conditions: LiquidityConditions {
                liquidity_score: 95.0,
                exchange_conditions: HashMap::new(),
            },
        },
        constraints: None,
    };

    let result = strategy.execute(&context).await;
    assert!(result.is_ok());

    let actions = result.unwrap();
    // Conservative strategy may not recommend actions if amounts are too small or APY too low

    // Verify strategy characteristics
    assert_eq!(strategy.name(), "Conservative");
    let risk_profile = strategy.risk_assessment();
    assert!(risk_profile.risk_level < 50); // Conservative strategy
    assert!(
        risk_profile.liquidity_requirements.min_flexible_percentage
            > Decimal::from_f64(0.5).unwrap()
    );
}

#[test]
fn test_diversified_strategy() {
    let strategy = DiversifiedStrategy::new();

    assert_eq!(strategy.name(), "Diversified");
    let risk_profile = strategy.risk_assessment();
    assert_eq!(risk_profile.risk_level, 50); // Moderate risk
    assert!(risk_profile.diversification.min_positions >= 5);
}

#[test]
fn test_liquidity_first_strategy() {
    let strategy = LiquidityFirstStrategy;

    assert_eq!(strategy.name(), "LiquidityFirst");
    let risk_profile = strategy.risk_assessment();
    assert!(risk_profile.risk_level < 20); // Very low risk
    assert_eq!(
        risk_profile.liquidity_requirements.min_flexible_percentage,
        Decimal::ONE
    ); // 100% flexible
}

#[test]
fn test_staking_types() {
    // Test different staking types
    let flexible = StakingType::Flexible;
    let locked = StakingType::Locked(Duration::days(90));
    let defi = StakingType::DeFi;
    let liquid = StakingType::Liquid;

    assert_eq!(flexible, StakingType::Flexible);
    assert_ne!(flexible, locked);
    assert_ne!(defi, liquid);
}

#[test]
fn test_staking_operations() {
    // Test staking operation creation
    let operation = StakingOperation {
        id: "test_op_1".to_string(),
        operation_type: StakingOperationType::Stake,
        exchange: ExchangeId::BinanceSpot,
        asset: "USDT".to_string(),
        amount: Decimal::from(1000),
        timestamp: Utc::now(),
        status: StakingOperationStatus::Success,
        error: None,
    };

    assert_eq!(operation.id, "test_op_1");
    assert_eq!(operation.operation_type, StakingOperationType::Stake);
    assert_eq!(operation.status, StakingOperationStatus::Success);
    assert!(operation.error.is_none());
}

#[test]
fn test_unified_staking_manager() {
    let mut unified_manager = UnifiedStakingManager::new();

    // Add mock managers using the enum wrapper
    let binance_config = BinanceStakingConfig::default();
    let binance_manager = BinanceStakingManager::new(binance_config);
    unified_manager.add_manager(StakingManagerImpl::Binance(binance_manager));

    let bybit_config = BybitStakingConfig::default();
    let bybit_manager = BybitStakingManager::new(bybit_config);
    unified_manager.add_manager(StakingManagerImpl::Bybit(bybit_manager));

    // Test supported exchanges
    let exchanges = unified_manager.supported_exchanges();
    assert!(exchanges.contains(&ExchangeId::BinanceSpot));
    assert!(exchanges.contains(&ExchangeId::BybitSpot));

    // Test getting manager
    let binance_mgr = unified_manager.get_manager(ExchangeId::BinanceSpot);
    assert!(binance_mgr.is_some());

    // Test that we have 2 managers
    assert_eq!(exchanges.len(), 2);
}

#[test]
fn test_staking_error_types() {
    let error = StakingError::InsufficientBalance {
        required: Decimal::from(1000),
        available: Decimal::from(500),
    };

    assert!(!error.is_recoverable());
    assert_eq!(
        error.severity(),
        jackbot_execution::staking::error::ErrorSeverity::Low
    );
    assert_eq!(
        error.category(),
        jackbot_execution::staking::error::ErrorCategory::Balance
    );

    let network_error = StakingError::NetworkError {
        message: "Connection timeout".to_string(),
    };

    assert!(network_error.is_recoverable());
    assert_eq!(
        network_error.severity(),
        jackbot_execution::staking::error::ErrorSeverity::Medium
    );
}

#[test]
fn test_optimization_params() {
    let default_params = OptimizationParams::default();
    assert!(default_params.yield_weight > Decimal::ZERO);
    assert!(default_params.risk_weight > Decimal::ZERO);
    assert!(default_params.liquidity_weight > Decimal::ZERO);
    assert!(default_params.max_products > 0);

    let custom_risk_settings = RiskSettings {
        max_exchange_exposure: Decimal::from_f64(0.2).unwrap(),
        max_asset_exposure: Decimal::from_f64(0.3).unwrap(),
        max_locked_exposure: Decimal::from_f64(0.1).unwrap(),
        min_liquidity_buffer: Decimal::from_f64(0.2).unwrap(),
        risk_tolerance: RiskTolerance::Conservative,
    };

    let optimizer = YieldOptimizer::with_settings(custom_risk_settings, default_params);
    assert!(optimizer.risk_settings.max_exchange_exposure < Decimal::from_f64(0.25).unwrap());
}

#[test]
fn test_allocation_strategy() {
    let mut exchange_allocations = HashMap::new();
    exchange_allocations.insert(ExchangeId::BinanceSpot, Decimal::from_f64(0.4).unwrap());
    exchange_allocations.insert(ExchangeId::BybitSpot, Decimal::from_f64(0.3).unwrap());
    exchange_allocations.insert(ExchangeId::Okx, Decimal::from_f64(0.3).unwrap());

    let mut asset_allocations = HashMap::new();
    asset_allocations.insert("USDT".to_string(), Decimal::from_f64(0.5).unwrap());
    asset_allocations.insert("BTC".to_string(), Decimal::from_f64(0.3).unwrap());
    asset_allocations.insert("ETH".to_string(), Decimal::from_f64(0.2).unwrap());

    let mut type_allocations = HashMap::new();
    type_allocations.insert(StakingType::Flexible, Decimal::from_f64(0.6).unwrap());
    type_allocations.insert(
        StakingType::Locked(Duration::days(90)),
        Decimal::from_f64(0.4).unwrap(),
    );

    let strategy = AllocationStrategy {
        exchange_allocations,
        asset_allocations,
        type_allocations,
        rebalance_frequency: Duration::days(7),
    };

    // Verify allocation totals are reasonable
    let total_exchange_allocation: Decimal = strategy.exchange_allocations.values().sum();
    assert!((total_exchange_allocation - Decimal::ONE).abs() < Decimal::from_f64(0.01).unwrap());

    let total_asset_allocation: Decimal = strategy.asset_allocations.values().sum();
    assert!((total_asset_allocation - Decimal::ONE).abs() < Decimal::from_f64(0.01).unwrap());
}

#[test]
fn test_staking_constraints() {
    let constraints = StakingConstraints {
        min_apy: Some(Decimal::from_f64(0.05).unwrap()), // 5%
        max_lock_period: Some(Duration::days(180)),
        preferred_types: vec![StakingType::Flexible, StakingType::Liquid],
        exchange_filter: ExchangeFilter::Include(vec![
            ExchangeId::BinanceSpot,
            ExchangeId::Coinbase,
        ]),
        risk_tolerance: RiskTolerance::Conservative,
    };

    // Test exchange filter
    match constraints.exchange_filter {
        ExchangeFilter::Include(ref exchanges) => {
            assert!(exchanges.contains(&ExchangeId::BinanceSpot));
            assert!(!exchanges.contains(&ExchangeId::BybitSpot));
        }
        _ => panic!("Expected Include filter"),
    }

    // Test constraints validation
    assert!(constraints.min_apy.is_some());
    assert!(constraints.max_lock_period.is_some());
    assert_eq!(constraints.risk_tolerance, RiskTolerance::Conservative);
}

// Integration test to verify the core staking workflow
#[tokio::test]
async fn test_basic_staking_workflow() {
    // 1. Create a unified staking manager
    let mut unified_manager = UnifiedStakingManager::new();

    // Add managers for different exchanges
    unified_manager.add_manager(StakingManagerImpl::Binance(BinanceStakingManager::new(
        BinanceStakingConfig::default(),
    )));
    unified_manager.add_manager(StakingManagerImpl::Bybit(BybitStakingManager::new(
        BybitStakingConfig::default(),
    )));

    // 2. Create yield optimizer
    let optimizer = YieldOptimizer::new();

    // 3. Create mock data
    let products = vec![
        create_mock_product(
            "product_1",
            "USDT",
            ExchangeId::BinanceSpot,
            5.0,
            StakingType::Flexible,
        ),
        create_mock_product(
            "product_2",
            "USDT",
            ExchangeId::BybitSpot,
            6.0,
            StakingType::Flexible,
        ),
    ];

    // 4. Test optimization
    let total_amount = Decimal::from(10000);
    let recommendations = optimizer.find_best_products(&products, total_amount, None);
    assert!(recommendations.is_ok());

    // 5. Get portfolio summary
    let portfolio_summary = unified_manager.get_portfolio_summary().await;
    assert!(portfolio_summary.is_ok());

    println!("Basic staking workflow test passed successfully!");
}
