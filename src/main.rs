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

use config::SensorConfig;
use sensor::SensorManager;

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
    /// Start the sensor system
    Start {
        /// Exchanges to monitor (comma-separated)
        #[arg(short, long, default_value = "binance")]
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
    },

    /// Run health check on sensor
    Health,

    /// Show sensor status
    Status,

    /// Run in MVP mode (simplified Redis order processing)
    Mvp,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize rustls crypto provider
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");
    
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

    info!("🚀 Jackbot Sensor v1.0.0 starting...");

    match cli.command {
        Commands::Start {
            exchanges,
            pairs,
            paper_trading,
            pairs_per_instance,
        } => {
            info!("📊 Starting sensor with exchanges: {}", exchanges);
            
            // Load configuration (use default for now)
            let mut config = SensorConfig::default();
            // Kafka configuration is already set in default config
            
            // Enable requested exchanges
            for exchange_name in exchanges.split(',') {
                if let Some(exchange_config) = config.exchanges.get_mut(exchange_name.trim()) {
                    exchange_config.enabled = true;
                }
            }

            // Create and start sensor manager
            let mut sensor_manager = SensorManager::new(config, cli.instance_id).await?;
            
            info!("🔌 Initializing exchange connections...");
            sensor_manager.start().await?;

            // Wait for shutdown signal
            tokio::signal::ctrl_c().await?;
            info!("🛑 Shutdown signal received");

            sensor_manager.shutdown().await?;
            info!("✅ Sensor stopped gracefully");
        }

        Commands::Health => {
            info!("🏥 Running health check...");
            // TODO: Implement health check
            info!("✅ Health check passed");
        }

        Commands::Status => {
            info!("📊 Sensor Status");
            // TODO: Implement status check
            info!("✅ Status check complete");
        }

        Commands::Mvp => {
            info!("🚀 Starting MVP mode (Kafka order processing)");
            mvp::run_mvp(&cli.kafka_brokers).await?;
        }
    }

    Ok(())
}