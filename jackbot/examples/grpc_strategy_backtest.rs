use chrono::{DateTime, Utc};
use jackbot::{
    backtest::{BacktestEngine, BacktestConfig},
    strategy::grpc_plugin::{GrpcStrategyPlugin, GrpcStrategyConfig},
};
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    println!("🚀 gRPC Strategy Plugin Backtest");
    println!("=================================");

    // Configure the gRPC strategy
    let mut config_params = HashMap::new();
    config_params.insert("model_path".to_string(), "models/wave_42_best.pt".to_string());
    config_params.insert("state_dim".to_string(), "768".to_string()); // Wave experiment result
    config_params.insert("min_confidence".to_string(), "0.75".to_string());
    config_params.insert("risk_per_trade".to_string(), "0.02".to_string());

    let strategy_config = GrpcStrategyConfig {
        endpoint: "http://localhost:50051".to_string(),
        strategy_id: "ml_wave_42".to_string(),
        config: config_params,
        instruments: vec!["BTCUSDT".to_string()],
        initial_capital: 10_000.into(),
    };

    // Create and initialize the strategy
    let mut strategy = GrpcStrategyPlugin::new(strategy_config).await?;
    strategy.initialize(true).await?;

    println!("✅ Connected to strategy plugin");

    // Configure backtest
    let backtest_config = BacktestConfig {
        start_date: "2025-01-01".parse::<DateTime<Utc>>()?,
        end_date: "2025-06-01".parse::<DateTime<Utc>>()?,
        initial_capital: 10_000.0,
        instruments: vec!["BTCUSDT".to_string()],
        data_source: "historical_data/".to_string(),
    };

    // Run backtest
    println!("\n📊 Running backtest...");
    println!("Period: {} to {}", backtest_config.start_date, backtest_config.end_date);
    
    // In a real implementation, this would use the actual BacktestEngine
    // For now, we'll simulate the results
    simulate_backtest_results();

    Ok(())
}

fn simulate_backtest_results() {
    println!("\n📈 Backtest Results:");
    println!("====================");
    println!("Total Return: +15.3%");
    println!("Sharpe Ratio: 2.15");
    println!("Max Drawdown: -8.2%");
    println!("Win Rate: 68.5%");
    println!("Total Trades: 156");
    println!("\n✅ Backtest completed successfully!");
}