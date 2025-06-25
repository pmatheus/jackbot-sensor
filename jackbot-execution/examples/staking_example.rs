//! Example demonstrating how to use the staking operations

use jackbot_execution::staking::{
    binance::{BinanceStakingConfig, BinanceStakingManager},
    bybit::{BybitStakingConfig, BybitStakingManager},
    manager::{StakingManagerImpl, UnifiedStakingManager},
    okx::{OKXStakingConfig, OKXStakingManager},
    optimizer::YieldOptimizer,
    strategies::{ConservativeStrategy, MaxYieldStrategy, StakingStrategy},
    ExchangeFilter, RiskTolerance, StakingConstraints, StakingManager, StakingType,
};
use jackbot_instrument::{asset::name::AssetNameExchange, exchange::ExchangeId};
use rust_decimal::Decimal;
use std::{collections::HashMap, str::FromStr};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Jackbot Staking Operations Example");

    // Create a unified staking manager
    let mut unified_manager = UnifiedStakingManager::new();

    // Add exchange-specific managers
    let binance_config = BinanceStakingConfig {
        api_key: "your_binance_api_key".to_string(),
        secret_key: "your_binance_secret".to_string(),
        base_url: "https://api.binance.com".to_string(),
        testnet: true, // Use testnet for development
    };
    unified_manager.add_manager(StakingManagerImpl::Binance(BinanceStakingManager::new(
        binance_config,
    )));

    let bybit_config = BybitStakingConfig {
        api_key: "your_bybit_api_key".to_string(),
        secret_key: "your_bybit_secret".to_string(),
        base_url: "https://api-testnet.bybit.com".to_string(),
        testnet: true,
    };
    unified_manager.add_manager(StakingManagerImpl::Bybit(BybitStakingManager::new(
        bybit_config,
    )));

    let okx_config = OKXStakingConfig {
        api_key: "your_okx_api_key".to_string(),
        secret_key: "your_okx_secret".to_string(),
        passphrase: "your_okx_passphrase".to_string(),
        base_url: "https://www.okx.com".to_string(),
        sandbox: true,
    };
    unified_manager.add_manager(StakingManagerImpl::Okx(OKXStakingManager::new(okx_config)));

    println!(
        "✅ Configured managers for {} exchanges",
        unified_manager.supported_exchanges().len()
    );

    // Create staking constraints
    let constraints = StakingConstraints {
        min_apy: Some(Decimal::from_str("0.03")?), // 3% minimum APY
        max_lock_period: Some(chrono::Duration::days(90)),
        preferred_types: vec![StakingType::Flexible, StakingType::Liquid],
        exchange_filter: ExchangeFilter::All,
        risk_tolerance: RiskTolerance::Moderate,
    };

    // Example: Get all staking positions
    println!("\n📊 Fetching staking positions...");
    match unified_manager.get_all_positions().await {
        Ok(positions) => {
            println!("Found {} active staking positions", positions.len());
            for position in positions.iter().take(3) {
                println!(
                    "  - {} {} on {} (APY: {:.2}%)",
                    position.amount,
                    position.asset,
                    position.exchange.as_str(),
                    position.product.apy * Decimal::from(100)
                );
            }
        }
        Err(e) => println!("Error fetching positions: {}", e),
    }

    // Example: Get portfolio summary
    println!("\n💰 Portfolio Summary:");
    match unified_manager.get_portfolio_summary().await {
        Ok(summary) => {
            println!("  Total Staked: ${}", summary.total_staked_value);
            println!("  Total Rewards: ${}", summary.total_accumulated_rewards);
            println!("  Available Rewards: ${}", summary.available_rewards);
            println!("  Active Positions: {}", summary.active_positions);

            println!("  Exchange Breakdown:");
            for (exchange, amount) in &summary.exchange_breakdown {
                println!("    {}: ${}", exchange.as_str(), amount);
            }
        }
        Err(e) => println!("Error getting portfolio summary: {}", e),
    }

    // Example: Yield optimization
    println!("\n🔍 Yield Optimization:");
    let optimizer = YieldOptimizer::new();

    // Note: In real usage, you would get these from the API
    let mock_products = vec![];
    let total_amount = Decimal::from_str("10000")?;

    match optimizer.find_best_products(&mock_products, total_amount, Some(&constraints)) {
        Ok(recommendations) => {
            println!("Found {} staking recommendations", recommendations.len());
            for rec in recommendations.iter().take(3) {
                println!(
                    "  - {} {} in {} (Expected: ${}/year, Risk: {})",
                    rec.amount,
                    rec.product.asset,
                    rec.product.id,
                    rec.expected_return,
                    rec.risk_score
                );
            }
        }
        Err(e) => println!("Error optimizing yield: {}", e),
    }

    // Example: Strategy execution
    println!("\n🎯 Strategy Execution:");

    let max_yield_strategy = MaxYieldStrategy::new();
    println!(
        "Strategy: {} (Risk Level: {})",
        max_yield_strategy.name(),
        max_yield_strategy.risk_assessment().risk_level
    );

    let conservative_strategy = ConservativeStrategy::new();
    println!(
        "Strategy: {} (Risk Level: {})",
        conservative_strategy.name(),
        conservative_strategy.risk_assessment().risk_level
    );

    // Example: Staking a specific asset
    println!("\n💸 Example Staking Operation:");
    let asset = AssetNameExchange::from("USDT");
    let amount = Decimal::from_str("1000")?;

    println!(
        "Attempting to stake {} {} across optimal exchanges...",
        amount, asset
    );
    match unified_manager
        .stake_optimized(&asset, amount, Some(constraints))
        .await
    {
        Ok(operations) => {
            println!("✅ Executed {} staking operations:", operations.len());
            for op in operations {
                println!(
                    "  - {} {} on {} (ID: {})",
                    op.amount,
                    op.asset,
                    op.exchange.as_str(),
                    op.id
                );
            }
        }
        Err(e) => println!("❌ Staking failed: {}", e),
    }

    println!("\n🎉 Staking operations example completed!");
    Ok(())
}
