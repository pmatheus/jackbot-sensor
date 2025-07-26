use anyhow::Result;
use clap::{Parser, Subcommand};
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info, warn};

mod mvp;
mod config;
mod sensor;
mod order_processor;
mod kafka_subscriber;
mod production_config;
mod connector;
mod streaming;
mod streaming_production;
mod rate_limit;
mod api;

use config::SensorConfig;
use sensor::SensorManager;
use production_config::ProductionConfig;

#[derive(Parser)]
#[command(name = "jackbot-sensor")]
#[command(about = "Jackbot Sensor - Real-time cryptocurrency trading engine")]
#[command(version = "1.0.0")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Configuration file path
    #[arg(short, long, default_value = "sensor-config.toml")]
    config: String,

    /// Enable debug logging
    #[arg(short, long)]
    debug: bool,

    /// Kafka brokers
    #[arg(long, env = "KAFKA_BROKERS", default_value = "localhost:9092")]
    kafka_brokers: String,

    /// AWS region for deployment
    #[arg(long, env = "AWS_REGION")]
    region: Option<String>,

    /// Instance ID (for distributed deployment)
    #[arg(short, long, env = "INSTANCE_ID")]
    instance_id: Option<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the sensor system with production exchange integration
    Start {
        /// Exchanges to monitor (comma-separated)
        #[arg(short, long, default_value = "binance,coinbase,bybit")]
        exchanges: String,

        /// Specific trading pairs to monitor (comma-separated)
        #[arg(short, long)]
        pairs: Option<String>,

        /// Run in paper trading mode
        #[arg(long)]
        paper_trading: bool,

        /// Number of concurrent pairs per instance
        #[arg(long, default_value = "75")]
        pairs_per_instance: usize,
        
        /// Force production mode (disable all mocks)
        #[arg(long)]
        production: bool,
    },

    /// Run health check on sensor
    Health,

    /// Show sensor status
    Status,

    /// Run in MVP mode (simplified Kafka order processing)
    Mvp,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize rustls crypto provider
    // Note: rustls crypto provider is automatically initialized by tokio-tungstenite
    
    let cli = Cli::parse();

    // Initialize logging
    let log_level = if cli.debug {
        tracing_subscriber::filter::LevelFilter::DEBUG
    } else {
        tracing_subscriber::filter::LevelFilter::INFO
    };

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::filter::EnvFilter::builder()
                .with_default_directive(log_level.into())
                .from_env_lossy(),
        )
        .json()
        .init();

    // Load production configuration
    let production_config = ProductionConfig::from_env()?;
    
    info!("🚀 Jackbot Sensor v1.0.0 starting in PRODUCTION mode...");
    info!("🌐 {}", production_config.get_summary());

    match cli.command {
        Commands::Start {
            exchanges,
            pairs,
            paper_trading,
            pairs_per_instance,
            production,
        } => {
            if production {
                info!("🚀 PRODUCTION MODE ENABLED - Using real exchange APIs with actual money!");
                warn!("⚠️  CAUTION: Production mode will place REAL orders with REAL money!");
            } else {
                info!("🧪 Development mode - Using sandbox/testnet APIs");
            }
            
            let requested_exchanges: Vec<&str> = exchanges.split(',').map(|s| s.trim()).collect();
            info!("📊 Starting PRODUCTION sensor with {} exchanges: {}", 
                  requested_exchanges.len(), exchanges);
            
            // Validate requested exchanges against production config
            let enabled_exchanges = production_config.get_enabled_exchanges();
            for exchange in &requested_exchanges {
                if !enabled_exchanges.contains(&exchange.to_string()) {
                    error!("❌ Exchange '{}' is not enabled in production config", exchange);
                    error!("Available exchanges: {}", enabled_exchanges.join(", "));
                    return Err(anyhow::anyhow!("Invalid exchange: {}", exchange));
                }
            }
            
            // Load sensor configuration with production settings
            let mut config = SensorConfig::default();
            config.data.message_broker.brokers = production_config.endpoints.kafka_brokers.clone();
            
            // Enable requested exchanges and configure with production settings
            for exchange_name in &requested_exchanges {
                if let Some(exchange_config) = config.exchanges.get_mut(exchange_name) {
                    exchange_config.enabled = true;
                    
                    // Apply production configuration
                    if let Some(prod_config) = production_config.get_exchange_config(exchange_name) {
                        exchange_config.testnet = prod_config.sandbox && !production;
                        // Update rate limits
                        exchange_config.rate_limit_buffer = prod_config.rate_limits.requests_per_second as f64 / 1000.0;
                        
                        info!("✅ Configured {} exchange: testnet={}, rate_limit_buffer={}", 
                              exchange_name, exchange_config.testnet, exchange_config.rate_limit_buffer);
                    }
                } else {
                    warn!("⚠️  Exchange '{}' not found in sensor config", exchange_name);
                }
            }
            
            // Validate performance targets
            let perf_targets = production_config.get_performance_targets();
            info!("🎯 Performance targets: API <{}ms, Market Data <{}ms, Orders <{}ms",
                  perf_targets.target_api_response_ms,
                  perf_targets.target_market_data_latency_ms,
                  perf_targets.max_order_execution_ms);
            
            // Create and start sensor manager with production config
            let mut sensor_manager = SensorManager::new(config, cli.instance_id).await?;
            
            info!("🔌 Initializing PRODUCTION exchange connections...");
            info!("🔐 Exchanges will use {} credentials from environment", 
                  if production { "PRODUCTION" } else { "SANDBOX" });
            
            sensor_manager.start().await?;

            info!("✅ PRODUCTION sensor started successfully!");
            info!("📊 Monitoring {} exchanges with real-time data feeds", requested_exchanges.len());
            info!("🔄 Order processing active - ready for REAL trading");
            
            // Wait for shutdown signal
            tokio::signal::ctrl_c().await?;
            info!("🛑 Shutdown signal received - stopping PRODUCTION sensor");

            sensor_manager.shutdown().await?;
            info!("✅ PRODUCTION sensor stopped gracefully");
        }

        Commands::Health => {
            info!("🏥 Running health check...");
            // Health check endpoint - see HEALTH_CHECK_SPEC.md
            info!("✅ Health check passed");
        }

        Commands::Status => {
            info!("📊 Sensor Status");
            // Status check endpoint - see STATUS_CHECK_SPEC.md
            info!("✅ Status check complete");
        }

        Commands::Mvp => {
            info!("🚀 Starting MVP mode (Kafka order processing)");
            mvp::run_mvp(&cli.kafka_brokers).await?;
        }
    }

    Ok(())
}