//! Integration test for the event-driven strategy framework
//!
//! This module demonstrates how the sensor strategy framework can be used
//! for high-frequency trading operations with sub-50ms evaluation times.

use crate::strategy::{
    events::{MarketEvent, TradeSide},
    sensor_manager::{
        SensorStrategyManager, SensorStrategyParameters, SensorStrategyRequest, SensorStrategyType,
    },
};
use jackbot_instrument::{exchange::ExchangeId, instrument::name::InstrumentNameExchange};
use rust_decimal::prelude::*;
use rust_decimal::Decimal;
use std::{collections::HashMap, time::Duration};
use tokio::time::sleep;
use tracing::{info, warn};

/// Demonstrates the complete sensor strategy framework
#[derive(Debug)]
pub struct StrategyFrameworkDemo {
    manager: SensorStrategyManager,
}

impl StrategyFrameworkDemo {
    /// Create a new demo instance
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let manager = SensorStrategyManager::new()?;
        Ok(Self { manager })
    }

    /// Run the complete demonstration
    pub async fn run_demo(&self) -> Result<(), Box<dyn std::error::Error>> {
        info!("Starting event-driven strategy framework demonstration");

        // Start the strategy manager
        self.manager.start().await?;

        // Create sample strategies
        self.create_sample_strategies().await?;

        // Simulate market events
        self.simulate_market_events().await?;

        // Monitor performance
        self.monitor_performance().await?;

        // Clean up
        self.cleanup().await?;

        info!("Event-driven strategy framework demonstration completed successfully");
        Ok(())
    }

    /// Create various types of sensor strategies
    async fn create_sample_strategies(&self) -> Result<(), Box<dyn std::error::Error>> {
        let exchange = ExchangeId::BinanceSpot;
        let instrument = InstrumentNameExchange::new("BTCUSDT");

        // Create TWAP strategy
        let twap_request = SensorStrategyRequest {
            strategy_id: "demo_twap_001".to_string(),
            strategy_type: SensorStrategyType::Twap,
            exchange: exchange.clone(),
            instrument: instrument.clone(),
            parameters: SensorStrategyParameters {
                target_quantity: Decimal::from_str("1.0").unwrap(),
                duration: Some(Duration::from_secs(300)), // 5 minutes
                slice_count: Some(10),
                participation_rate: None,
                chunk_size: None,
                max_concurrent_orders: None,
                assessment_interval: None,
                custom_params: HashMap::new(),
            },
        };

        let strategy_id = self.manager.create_strategy(twap_request).await?;
        info!(strategy_id = %strategy_id, "Created TWAP strategy");

        // Create VWAP strategy
        let vwap_request = SensorStrategyRequest {
            strategy_id: "demo_vwap_001".to_string(),
            strategy_type: SensorStrategyType::Vwap,
            exchange: exchange.clone(),
            instrument: instrument.clone(),
            parameters: SensorStrategyParameters {
                target_quantity: Decimal::from_str("2.0").unwrap(),
                duration: None,
                slice_count: None,
                participation_rate: Some(0.15), // 15% participation
                chunk_size: None,
                max_concurrent_orders: None,
                assessment_interval: None,
                custom_params: HashMap::new(),
            },
        };

        let strategy_id = self.manager.create_strategy(vwap_request).await?;
        info!(strategy_id = %strategy_id, "Created VWAP strategy");

        // Create Iceberg strategy
        let iceberg_request = SensorStrategyRequest {
            strategy_id: "demo_iceberg_001".to_string(),
            strategy_type: SensorStrategyType::Iceberg,
            exchange: exchange.clone(),
            instrument: instrument.clone(),
            parameters: SensorStrategyParameters {
                target_quantity: Decimal::from_str("5.0").unwrap(),
                duration: None,
                slice_count: None,
                participation_rate: None,
                chunk_size: Some(Decimal::from_str("0.5").unwrap()),
                max_concurrent_orders: Some(3),
                assessment_interval: None,
                custom_params: HashMap::new(),
            },
        };

        let strategy_id = self.manager.create_strategy(iceberg_request).await?;
        info!(strategy_id = %strategy_id, "Created Iceberg strategy");

        // Create POV strategy
        let pov_request = SensorStrategyRequest {
            strategy_id: "demo_pov_001".to_string(),
            strategy_type: SensorStrategyType::Pov,
            exchange: exchange.clone(),
            instrument: instrument.clone(),
            parameters: SensorStrategyParameters {
                target_quantity: Decimal::from_str("3.0").unwrap(),
                duration: None,
                slice_count: None,
                participation_rate: Some(0.10), // 10% participation
                chunk_size: None,
                max_concurrent_orders: None,
                assessment_interval: Some(Duration::from_secs(30)),
                custom_params: HashMap::new(),
            },
        };

        let strategy_id = self.manager.create_strategy(pov_request).await?;
        info!(strategy_id = %strategy_id, "Created POV strategy");

        Ok(())
    }

    /// Simulate realistic market events for testing
    async fn simulate_market_events(&self) -> Result<(), Box<dyn std::error::Error>> {
        info!("Starting market event simulation");

        let exchange = ExchangeId::BinanceSpot;
        let instrument = InstrumentNameExchange::new("BTCUSDT");

        // Simulate order book updates
        for i in 0..50 {
            let base_price = Decimal::from_str("45000.0").unwrap();
            let price_offset = Decimal::from_str("10.0").unwrap() * Decimal::from(i);

            let bid_price = base_price - price_offset;
            let ask_price = base_price + price_offset;

            let event = MarketEvent::OrderBookUpdate {
                exchange: exchange.clone(),
                instrument: instrument.clone(),
                timestamp: chrono::Utc::now().timestamp_millis() as u64,
                bids: vec![
                    (bid_price, Decimal::from_str("1.5").unwrap()),
                    (
                        bid_price - Decimal::from_str("5.0").unwrap(),
                        Decimal::from_str("2.0").unwrap(),
                    ),
                    (
                        bid_price - Decimal::from_str("10.0").unwrap(),
                        Decimal::from_str("3.0").unwrap(),
                    ),
                ],
                asks: vec![
                    (ask_price, Decimal::from_str("1.2").unwrap()),
                    (
                        ask_price + Decimal::from_str("5.0").unwrap(),
                        Decimal::from_str("1.8").unwrap(),
                    ),
                    (
                        ask_price + Decimal::from_str("10.0").unwrap(),
                        Decimal::from_str("2.5").unwrap(),
                    ),
                ],
            };

            self.manager.publish_event(event).await?;

            // Small delay to simulate realistic timing
            sleep(Duration::from_millis(10)).await;
        }

        // Simulate trades
        for i in 0..20 {
            let trade_price = Decimal::from_str("45000.0").unwrap()
                + Decimal::from_str("5.0").unwrap() * Decimal::from(i % 10);
            let trade_volume = Decimal::from_str("0.1").unwrap()
                + Decimal::from_str("0.05").unwrap() * Decimal::from(i % 3);

            let event = MarketEvent::Trade {
                exchange: exchange.clone(),
                instrument: instrument.clone(),
                timestamp: chrono::Utc::now().timestamp_millis() as u64,
                price: trade_price,
                volume: trade_volume,
                side: if i % 2 == 0 {
                    TradeSide::Buy
                } else {
                    TradeSide::Sell
                },
            };

            self.manager.publish_event(event).await?;
            sleep(Duration::from_millis(20)).await;
        }

        // Simulate volume spike
        let volume_spike_event = MarketEvent::VolumeSpike {
            exchange: exchange.clone(),
            instrument: instrument.clone(),
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
            volume: Decimal::from_str("10.0").unwrap(),
            threshold_multiplier: 3.0,
        };

        self.manager.publish_event(volume_spike_event).await?;

        info!("Market event simulation completed");
        Ok(())
    }

    /// Monitor strategy performance
    async fn monitor_performance(&self) -> Result<(), Box<dyn std::error::Error>> {
        info!("Monitoring strategy performance");

        // Wait for strategies to process events
        sleep(Duration::from_secs(2)).await;

        // Check strategy status
        let strategies = self.manager.list_strategies().await;
        info!(active_strategies = strategies.len(), "Active strategies");

        for (strategy_id, info) in &strategies {
            info!(
                strategy_id = %strategy_id,
                strategy_type = %info.strategy_type,
                signals_generated = info.signals_generated,
                avg_execution_time_us = info.avg_execution_time_us,
                status = ?info.status,
                "Strategy performance"
            );
        }

        // Check manager metrics
        let metrics = self.manager.get_metrics();
        info!(
            strategies_created = metrics
                .strategies_created
                .load(std::sync::atomic::Ordering::Relaxed),
            strategies_active = metrics
                .strategies_active
                .load(std::sync::atomic::Ordering::Relaxed),
            total_signals_processed = metrics
                .total_signals_processed
                .load(std::sync::atomic::Ordering::Relaxed),
            total_events_processed = metrics
                .total_events_processed
                .load(std::sync::atomic::Ordering::Relaxed),
            avg_signal_processing_time_us = metrics
                .avg_signal_processing_time_us
                .load(std::sync::atomic::Ordering::Relaxed),
            error_count = metrics
                .error_count
                .load(std::sync::atomic::Ordering::Relaxed),
            uptime_seconds = metrics.start_time.elapsed().as_secs(),
            "Manager performance metrics"
        );

        // Validate performance targets
        let avg_processing_time = metrics
            .avg_signal_processing_time_us
            .load(std::sync::atomic::Ordering::Relaxed);
        let target_time_us = 50_000; // 50ms in microseconds

        if avg_processing_time > target_time_us {
            warn!(
                avg_processing_time_us = avg_processing_time,
                target_time_us = target_time_us,
                "Performance target not met - exceeding 50ms evaluation time"
            );
        } else {
            info!(
                avg_processing_time_us = avg_processing_time,
                target_time_us = target_time_us,
                "Performance target met - within 50ms evaluation time"
            );
        }

        // Health check
        let is_healthy = self.manager.health_check().await;
        info!(is_healthy = is_healthy, "Strategy manager health check");

        Ok(())
    }

    /// Clean up demo resources
    async fn cleanup(&self) -> Result<(), Box<dyn std::error::Error>> {
        info!("Cleaning up demo resources");

        // List all strategies and remove them
        let strategies = self.manager.list_strategies().await;
        for strategy_id in strategies.keys() {
            if let Err(e) = self.manager.remove_strategy(strategy_id).await {
                warn!(
                    strategy_id = %strategy_id,
                    error = %e,
                    "Failed to remove strategy during cleanup"
                );
            } else {
                info!(strategy_id = %strategy_id, "Removed strategy");
            }
        }

        // Shutdown the manager
        self.manager.shutdown().await?;

        info!("Demo cleanup completed");
        Ok(())
    }
}

/// Performance test for the strategy framework
pub async fn run_performance_test() -> Result<(), Box<dyn std::error::Error>> {
    info!("Starting performance test for event-driven strategy framework");

    let demo = StrategyFrameworkDemo::new()?;

    // Measure total execution time
    let start_time = std::time::Instant::now();

    // Run the demonstration
    demo.run_demo().await?;

    let total_time = start_time.elapsed();

    info!(
        total_execution_time_ms = total_time.as_millis(),
        "Performance test completed"
    );

    // Performance assertions
    assert!(
        total_time < Duration::from_secs(30),
        "Demo should complete within 30 seconds"
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_strategy_framework_demo() {
        // Initialize logging for tests
        let _ = tracing_subscriber::fmt::try_init();

        let result = run_performance_test().await;
        assert!(result.is_ok(), "Performance test should pass: {:?}", result);
    }

    #[tokio::test]
    async fn test_individual_strategies() {
        let _ = tracing_subscriber::fmt::try_init();

        let manager = SensorStrategyManager::new().expect("Should create manager");
        manager.start().await.expect("Should start manager");

        // Test TWAP strategy creation
        let twap_id = manager
            .create_twap_strategy(
                "test_twap".to_string(),
                ExchangeId::BinanceSpot,
                InstrumentNameExchange::new("ETHUSDT"),
                Decimal::from_str("1.0").unwrap(),
                Duration::from_secs(60),
            )
            .await
            .expect("Should create TWAP strategy");

        assert_eq!(twap_id, "test_twap");

        // Test strategy status
        let status = manager.get_strategy_status(&twap_id).await;
        assert!(status.is_some(), "Strategy should exist");

        // Cleanup
        manager
            .remove_strategy(&twap_id)
            .await
            .expect("Should remove strategy");
        manager.shutdown().await.expect("Should shutdown manager");
    }
}
