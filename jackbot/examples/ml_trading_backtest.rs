use Jackbot::{
    backtest::{
        BacktestArgsConstant, BacktestArgsDynamic, data_loader::DataLoader,
        market_data::FileReplayMarketData, run_backtests,
    },
    engine::state::EngineState,
    risk::DefaultRisk,
    statistic::time::Period,
    strategy::ml_trading::{MlTradingConfig, MlTradingStrategy},
    system::config::ExecutionConfig,
};
use jackbot_data::exchange::{binance::market::BinanceMarketBuilder, coinbase::market::CoinbaseMarketBuilder};
use jackbot_instrument::{exchange::ExchangeId, index::IndexBuilder, instrument::kind::InstrumentKind};
use rust_decimal_macros::dec;
use std::{path::PathBuf, sync::Arc};
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    // Start the ML inference service first
    info!("Starting ML inference service...");
    info!("Make sure to run: python src/microservices/model_inference_service.py");
    
    // Configure instruments
    let mut index_builder = IndexBuilder::new();
    
    // Add Binance BTC/USDT perpetual
    index_builder.add_instrument(
        BinanceMarketBuilder::new(ExchangeId::Binance, InstrumentKind::Perpetual)
            .btc_usdt()
            .build()?
    )?;
    
    // Add Coinbase BTC/USD spot
    index_builder.add_instrument(
        CoinbaseMarketBuilder::new(ExchangeId::Coinbase, InstrumentKind::Spot)
            .btc_usd()
            .build()?
    )?;
    
    let instruments = index_builder.build();
    
    // Configure market data loader
    let data_loader = DataLoader::new(vec![
        PathBuf::from("data/binance/btcusdt_perp_2025_01.njson"),
        PathBuf::from("data/binance/btcusdt_perp_2025_02.njson"),
        PathBuf::from("data/binance/btcusdt_perp_2025_03.njson"),
        PathBuf::from("data/binance/btcusdt_perp_2025_04.njson"),
        PathBuf::from("data/binance/btcusdt_perp_2025_05.njson"),
        PathBuf::from("data/binance/btcusdt_perp_2025_06.njson"),
    ]);
    
    let market_data = FileReplayMarketData::new(data_loader);
    
    // Configure execution
    let executions = vec![
        ExecutionConfig::Mock(Default::default()),
    ];
    
    // Configure backtest constants
    let args_constant = Arc::new(BacktestArgsConstant {
        instruments,
        executions,
        market_data,
        summary_interval: Period::Daily,
        engine_state: EngineState::default(),
    });
    
    // Configure ML trading strategies with different parameters
    let strategies = vec![
        // Conservative strategy - high confidence threshold
        BacktestArgsDynamic {
            id: "ml_conservative".into(),
            risk_free_return: dec!(0.05),
            strategy: MlTradingStrategy::new(MlTradingConfig {
                api_url: "http://localhost:8011".to_string(),
                model_id: "ensemble".to_string(),
                position_size: dec!(0.01),
                min_confidence: 0.85,
                stop_loss_pct: dec!(0.02),
                take_profit_pct: dec!(0.04),
                log_activations: false,
            }),
            risk: DefaultRisk,
        },
        // Moderate strategy - balanced approach
        BacktestArgsDynamic {
            id: "ml_moderate".into(),
            risk_free_return: dec!(0.05),
            strategy: MlTradingStrategy::new(MlTradingConfig {
                api_url: "http://localhost:8011".to_string(),
                model_id: "ensemble".to_string(),
                position_size: dec!(0.02),
                min_confidence: 0.75,
                stop_loss_pct: dec!(0.025),
                take_profit_pct: dec!(0.05),
                log_activations: false,
            }),
            risk: DefaultRisk,
        },
        // Aggressive strategy - lower confidence threshold
        BacktestArgsDynamic {
            id: "ml_aggressive".into(),
            risk_free_return: dec!(0.05),
            strategy: MlTradingStrategy::new(MlTradingConfig {
                api_url: "http://localhost:8011".to_string(),
                model_id: "ensemble".to_string(),
                position_size: dec!(0.03),
                min_confidence: 0.65,
                stop_loss_pct: dec!(0.03),
                take_profit_pct: dec!(0.06),
                log_activations: true,
            }),
            risk: DefaultRisk,
        },
        // QR-DQN strategy
        BacktestArgsDynamic {
            id: "ml_qr_dqn".into(),
            risk_free_return: dec!(0.05),
            strategy: MlTradingStrategy::new(MlTradingConfig {
                api_url: "http://localhost:8011".to_string(),
                model_id: "qr_dqn".to_string(),
                position_size: dec!(0.02),
                min_confidence: 0.70,
                stop_loss_pct: dec!(0.025),
                take_profit_pct: dec!(0.05),
                log_activations: false,
            }),
            risk: DefaultRisk,
        },
    ];
    
    // Run backtests
    info!("Running ML trading backtests...");
    let results = run_backtests(args_constant, strategies).await?;
    
    // Display results
    info!("Backtest Results:");
    info!("{}", results);
    
    // Save detailed results
    let report_path = "reports/ml_trading_backtest_results.json";
    std::fs::write(
        report_path,
        serde_json::to_string_pretty(&results)?
    )?;
    info!("Detailed results saved to: {}", report_path);
    
    Ok(())
}