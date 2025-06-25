use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tokio::task::JoinHandle;
use tokio::time::{interval, Duration};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::api::ApiServer;
use crate::config::SensorConfig;
use crate::discovery::PairDiscovery;
use crate::distribution::PairDistributor;
use crate::monitor::{HealthMonitor, SystemDiagnostics};
use crate::streaming::{StreamingManager, Subscription};

use jackbot_data::subscription::Subscription as DataSubscription;
use jackbot_execution::client::mock::MockExecutionConfig;
use jackbot_risk::position_tracker::PositionTracker;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceInfo {
    pub id: String,
    pub region: String,
    pub instance_type: String,
    pub assigned_pairs: Vec<String>,
    pub cpu_usage: f64,
    pub memory_usage: f64,
    pub status: InstanceStatus,
    pub last_heartbeat: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InstanceStatus {
    Healthy,
    Degraded,
    Unhealthy,
    Terminated,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewPairAlert {
    pub exchange: String,
    pub symbol: String,
    pub base_asset: String,
    pub quote_asset: String,
    pub detected_at: chrono::DateTime<chrono::Utc>,
    pub detection_method: DetectionMethod,
    pub trading_start_time: Option<chrono::DateTime<chrono::Utc>>,
    pub priority: AlertPriority,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DetectionMethod {
    ApiPoll,
    WebSocket,
    Announcement,
    Social,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlertPriority {
    Critical,
    High,
    Medium,
    Low,
}

pub struct SensorManager {
    config: SensorConfig,
    instance_id: String,
    instances: Arc<RwLock<HashMap<String, InstanceInfo>>>,
    health_monitor: HealthMonitor,
    pair_discovery: PairDiscovery,
    pair_distributor: PairDistributor,
    api_server: Option<ApiServer>,
    exchange_subscriptions: HashMap<String, Subscription>,
    position_tracker: PositionTracker,
    alert_channel: mpsc::UnboundedSender<NewPairAlert>,
    alert_receiver: Option<mpsc::UnboundedReceiver<NewPairAlert>>,
    shutdown_signal: Option<tokio::sync::oneshot::Sender<()>>,
}

impl SensorManager {
    pub async fn new(config: SensorConfig, instance_id: Option<String>) -> Result<Self> {
        let instance_id = instance_id.unwrap_or_else(|| Uuid::new_v4().to_string());
        let (alert_tx, alert_rx) = mpsc::unbounded_channel();

        info!("Initializing sensor manager for instance: {}", instance_id);

        let health_monitor = HealthMonitor::new(config.monitoring.clone()).await?;
        let pair_discovery = PairDiscovery::new(config.discovery.clone()).await?;
        let pair_distributor = PairDistributor::new(config.deployment.clone()).await?;
        let position_tracker = PositionTracker::new();

        Ok(Self {
            config,
            instance_id,
            instances: Arc::new(RwLock::new(HashMap::new())),
            health_monitor,
            pair_discovery,
            pair_distributor,
            api_server: None,
            exchange_subscriptions: HashMap::new(),
            position_tracker,
            alert_channel: alert_tx,
            alert_receiver: Some(alert_rx),
            shutdown_signal: None,
        })
    }

    pub async fn start(
        &mut self,
        exchanges: Vec<String>,
        pairs: Vec<String>,
        paper_trading: bool,
        pairs_per_instance: usize,
    ) -> Result<()> {
        info!(
            "Starting sensor with {} exchanges, {} pairs",
            exchanges.len(),
            pairs.len()
        );

        // Initialize this instance
        self.initialize_instance(pairs_per_instance).await?;

        // Start exchange connections
        self.start_exchange_connections(exchanges, pairs, paper_trading)
            .await?;

        // Start API server
        self.start_api_server().await?;

        // Start background tasks
        self.start_background_tasks().await?;

        info!("Sensor started successfully");
        Ok(())
    }

    async fn initialize_instance(&mut self, pairs_per_instance: usize) -> Result<()> {
        let instance_info = InstanceInfo {
            id: self.instance_id.clone(),
            region: self.config.deployment.region.clone(),
            instance_type: self.config.deployment.instance_type.clone(),
            assigned_pairs: Vec::new(),
            cpu_usage: 0.0,
            memory_usage: 0.0,
            status: InstanceStatus::Healthy,
            last_heartbeat: chrono::Utc::now(),
        };

        self.instances
            .write()
            .await
            .insert(self.instance_id.clone(), instance_info);

        info!(
            "Initialized instance {} with capacity for {} pairs",
            self.instance_id, pairs_per_instance
        );
        Ok(())
    }

    async fn start_exchange_connections(
        &mut self,
        exchanges: Vec<String>,
        pairs: Vec<String>,
        paper_trading: bool,
    ) -> Result<()> {
        let enabled_exchanges = if exchanges.is_empty() {
            self.config.get_enabled_exchanges()
        } else {
            exchanges
        };

        for exchange in enabled_exchanges {
            if let Some(exchange_config) = self.config.exchanges.get(&exchange) {
                if !exchange_config.enabled {
                    continue;
                }

                info!("Starting connection to {}", exchange);

                // Create exchange subscription
                let subscription = self.create_exchange_subscription(&exchange, &pairs).await?;
                self.exchange_subscriptions
                    .insert(exchange.clone(), subscription);

                // Initialize trading client (paper or live)
                if paper_trading {
                    info!("Using paper trading for {}", exchange);
                    // Initialize mock client for paper trading
                } else if exchange_config.api_key.is_some() {
                    info!("Using live trading for {}", exchange);
                    // Initialize live trading client
                } else {
                    warn!("No API credentials for {}, market data only", exchange);
                }
            }
        }

        Ok(())
    }

    async fn create_exchange_subscription(
        &self,
        exchange: &str,
        pairs: &[String],
    ) -> Result<Subscription> {
        // Create a basic subscription for market data
        // This is a simplified implementation - in practice this would use jackbot-data patterns
        info!(
            "Creating subscription for {} with {} pairs",
            exchange,
            pairs.len()
        );

        // For now, use the basic Subscription structure from streaming module
        Ok(Subscription {
            channel: format!("market_data:{}:{}", exchange, pairs.join(",")),
            connection_id: format!("sensor_{}", uuid::Uuid::new_v4()),
            user_id: Some("sensor_system".to_string()),
            created_at: chrono::Utc::now().timestamp_millis(),
        })
    }

    async fn start_api_server(&mut self) -> Result<()> {
        let api_server = ApiServer::new(
            self.config.api.clone(),
            self.instances.clone(),
            self.alert_channel.clone(),
        )
        .await?;

        self.api_server = Some(api_server);
        info!(
            "API server started on ports REST:{}, WS:{}, Admin:{}",
            self.config.api.rest_port, self.config.api.websocket_port, self.config.api.admin_port
        );
        Ok(())
    }

    async fn start_background_tasks(&mut self) -> Result<()> {
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        self.shutdown_signal = Some(shutdown_tx);

        // Health monitoring task
        let health_monitor = self.health_monitor.clone();
        let instances = self.instances.clone();
        let instance_id = self.instance_id.clone();
        let health_interval = self.config.deployment.health_check_interval;

        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(health_interval));
            loop {
                interval.tick().await;

                if let Err(e) =
                    Self::update_instance_health(&health_monitor, &instances, &instance_id).await
                {
                    error!("Health update failed: {}", e);
                }
            }
        });

        // Pair discovery task
        if self.config.discovery.check_interval > 0 {
            let discovery = self.pair_discovery.clone();
            let alert_channel = self.alert_channel.clone();
            let discovery_interval = self.config.discovery.check_interval;

            tokio::spawn(async move {
                let mut interval = interval(Duration::from_secs(discovery_interval));
                loop {
                    interval.tick().await;

                    if let Err(e) = Self::run_pair_discovery(&discovery, &alert_channel).await {
                        error!("Pair discovery failed: {}", e);
                    }
                }
            });
        }

        // Alert processing task
        if let Some(mut alert_rx) = self.alert_receiver.take() {
            tokio::spawn(async move {
                while let Some(alert) = alert_rx.recv().await {
                    if let Err(e) = Self::process_new_pair_alert(alert).await {
                        error!("Alert processing failed: {}", e);
                    }
                }
            });
        }

        info!("Background tasks started");
        Ok(())
    }

    async fn update_instance_health(
        health_monitor: &HealthMonitor,
        instances: &Arc<RwLock<HashMap<String, InstanceInfo>>>,
        instance_id: &str,
    ) -> Result<()> {
        let metrics = health_monitor.get_system_metrics().await?;

        let mut instances_guard = instances.write().await;
        if let Some(instance) = instances_guard.get_mut(instance_id) {
            instance.cpu_usage = metrics.cpu_usage;
            instance.memory_usage = metrics.memory_usage;
            instance.last_heartbeat = chrono::Utc::now();

            // Update status based on metrics
            instance.status = if metrics.cpu_usage > 90.0 || metrics.memory_usage > 95.0 {
                InstanceStatus::Unhealthy
            } else if metrics.cpu_usage > 80.0 || metrics.memory_usage > 85.0 {
                InstanceStatus::Degraded
            } else {
                InstanceStatus::Healthy
            };
        }

        Ok(())
    }

    async fn run_pair_discovery(
        discovery: &PairDiscovery,
        alert_channel: &mpsc::UnboundedSender<NewPairAlert>,
    ) -> Result<()> {
        let new_pairs = discovery.discover_new_pairs().await?;

        for alert in new_pairs {
            if let Err(e) = alert_channel.send(alert) {
                error!("Failed to send new pair alert: {}", e);
            }
        }

        Ok(())
    }

    async fn process_new_pair_alert(alert: NewPairAlert) -> Result<()> {
        info!(
            "Processing new pair alert: {} on {}",
            alert.symbol, alert.exchange
        );

        // TODO: Implement alert processing
        // - Send notifications
        // - Update pair distribution
        // - Start monitoring new pair
        // - Alert trading strategies

        Ok(())
    }

    pub async fn run(&self) -> Result<()> {
        info!("Sensor is running...");

        // Keep the sensor running until shutdown
        if let Some(api_server) = &self.api_server {
            api_server.run().await?;
        }

        Ok(())
    }

    pub async fn run_discovery(&self) -> Result<()> {
        info!("Running in discovery mode...");

        // Discovery mode - just run pair discovery
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    }

    pub async fn run_monitoring(&self) -> Result<()> {
        info!("Running monitoring server...");

        if let Some(api_server) = &self.api_server {
            api_server.run_monitoring_only().await?;
        }

        Ok(())
    }

    pub async fn start_discovery(&mut self, exchange: &str, interval: u64) -> Result<()> {
        info!(
            "Starting discovery for {} with {}s interval",
            exchange, interval
        );
        self.pair_discovery
            .start_discovery(exchange, interval)
            .await
    }

    pub async fn start_monitoring(&mut self, port: u16, prometheus: bool) -> Result<()> {
        self.health_monitor
            .start_metrics_server(port, prometheus)
            .await
    }

    pub async fn distribute_pairs(&mut self, instances: usize, rebalance: bool) -> Result<()> {
        info!(
            "Distributing pairs across {} instances (rebalance: {})",
            instances, rebalance
        );

        let all_pairs = self.get_all_monitored_pairs().await?;
        let distribution = self
            .pair_distributor
            .distribute_pairs(all_pairs, instances)
            .await?;

        // Apply distribution
        for (instance_id, pairs) in distribution {
            info!("Instance {}: {} pairs", instance_id, pairs.len());
            // TODO: Apply the distribution
        }

        Ok(())
    }

    pub async fn scale_instances(&mut self, target: usize, immediate: bool) -> Result<()> {
        info!("Scaling to {} instances (immediate: {})", target, immediate);

        let current_count = self.instances.read().await.len();

        if target > current_count {
            info!("Scaling up by {} instances", target - current_count);
            // TODO: Implement scale up logic
        } else if target < current_count {
            info!("Scaling down by {} instances", current_count - target);
            // TODO: Implement scale down logic
        } else {
            info!("Already at target instance count");
        }

        Ok(())
    }

    pub async fn emergency_stop(&mut self) -> Result<()> {
        warn!("EMERGENCY STOP - Halting all trading activities");

        // Stop all trading clients
        for (exchange, _) in &self.exchange_subscriptions {
            warn!("Emergency stop for {}", exchange);
            // TODO: Emergency stop implementation
        }

        // Send emergency alert
        let alert = NewPairAlert {
            exchange: "SYSTEM".to_string(),
            symbol: "EMERGENCY_STOP".to_string(),
            base_asset: "SYSTEM".to_string(),
            quote_asset: "ALERT".to_string(),
            detected_at: chrono::Utc::now(),
            detection_method: DetectionMethod::Announcement,
            trading_start_time: None,
            priority: AlertPriority::Critical,
        };

        let _ = self.alert_channel.send(alert);

        Ok(())
    }

    pub async fn restart_connector(&mut self, exchange: &str) -> Result<()> {
        info!("Restarting connector for {}", exchange);

        if let Some(subscription) = self.exchange_subscriptions.remove(exchange) {
            // Stop existing subscription
            drop(subscription);

            // Recreate subscription
            let new_subscription = self.create_exchange_subscription(exchange, &[]).await?;
            self.exchange_subscriptions
                .insert(exchange.to_string(), new_subscription);

            info!("Successfully restarted connector for {}", exchange);
        } else {
            warn!("No active connector found for {}", exchange);
        }

        Ok(())
    }

    pub async fn export_logs(
        &self,
        start_time: Option<String>,
        end_time: Option<String>,
        format: &str,
    ) -> Result<()> {
        info!("Exporting logs in {} format", format);
        // TODO: Implement log export
        Ok(())
    }

    pub async fn get_diagnostics(&self) -> Result<SystemDiagnostics> {
        self.health_monitor.get_system_diagnostics().await
    }

    pub async fn update_symbols(&mut self, exchange: &str) -> Result<()> {
        info!("Updating symbols for {}", exchange);

        // Trigger pair discovery for specific exchange
        self.pair_discovery
            .update_exchange_symbols(exchange)
            .await?;

        Ok(())
    }

    async fn get_all_monitored_pairs(&self) -> Result<Vec<String>> {
        // Collect all pairs currently being monitored
        let mut all_pairs = Vec::new();

        for instance in self.instances.read().await.values() {
            all_pairs.extend(instance.assigned_pairs.clone());
        }

        all_pairs.sort();
        all_pairs.dedup();

        Ok(all_pairs)
    }

    pub async fn shutdown(&mut self) -> Result<()> {
        info!("Shutting down sensor manager...");

        // Signal shutdown to background tasks
        if let Some(shutdown_tx) = self.shutdown_signal.take() {
            let _ = shutdown_tx.send(());
        }

        // Shutdown exchange connections
        for (exchange, subscription) in self.exchange_subscriptions.drain() {
            info!("Shutting down {} connection", exchange);
            drop(subscription);
        }

        // Shutdown API server
        if let Some(api_server) = self.api_server.take() {
            api_server.shutdown().await?;
        }

        info!("Sensor manager shutdown complete");
        Ok(())
    }
}
