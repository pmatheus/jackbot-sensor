use anyhow::Result;
use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{interval, Duration, Instant};
use tracing::{info, warn, error};

use crate::config::MonitoringConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemMetrics {
    pub cpu_usage: f64,
    pub memory_usage: f64,
    pub network_usage: NetworkMetrics,
    pub disk_usage: f64,
    pub load_average: f64,
    pub uptime: u64,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkMetrics {
    pub inbound_bytes_per_sec: f64,
    pub outbound_bytes_per_sec: f64,
    pub connections_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyMetrics {
    pub p50: f64,
    pub p95: f64,
    pub p99: f64,
    pub average: f64,
    pub min: f64,
    pub max: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThroughputMetrics {
    pub messages_per_second: f64,
    pub orders_per_second: f64,
    pub trades_per_second: f64,
    pub websocket_messages_per_second: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorMetrics {
    pub total_errors: u64,
    pub error_rate: f64,
    pub errors_by_type: HashMap<String, u64>,
    pub last_error_time: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExchangeMetrics {
    pub exchange: String,
    pub connection_status: String,
    pub latency: LatencyMetrics,
    pub throughput: ThroughputMetrics,
    pub errors: ErrorMetrics,
    pub pairs_monitored: usize,
    pub last_update: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SystemDiagnostics {
    pub system: SystemMetrics,
    pub exchanges: Vec<ExchangeMetrics>,
    pub sensor_specific: SensorDiagnostics,
    pub alerts: Vec<Alert>,
    pub generated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SensorDiagnostics {
    pub instance_id: String,
    pub instance_count: usize,
    pub total_pairs_monitored: usize,
    pub pairs_per_instance_avg: f64,
    pub coverage_percentage: f64,
    pub new_pairs_detected_24h: usize,
    pub failed_connections: usize,
    pub memory_usage_per_pair: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    pub id: String,
    pub level: AlertLevel,
    pub title: String,
    pub description: String,
    pub source: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub resolved: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlertLevel {
    Critical,
    Warning,
    Info,
}

#[derive(Clone)]
pub struct HealthMonitor {
    config: MonitoringConfig,
    metrics: Arc<RwLock<SystemMetrics>>,
    exchange_metrics: Arc<RwLock<HashMap<String, ExchangeMetrics>>>,
    alerts: Arc<RwLock<Vec<Alert>>>,
    start_time: Instant,
}

impl HealthMonitor {
    pub async fn new(config: MonitoringConfig) -> Result<Self> {
        let initial_metrics = SystemMetrics {
            cpu_usage: 0.0,
            memory_usage: 0.0,
            network_usage: NetworkMetrics {
                inbound_bytes_per_sec: 0.0,
                outbound_bytes_per_sec: 0.0,
                connections_count: 0,
            },
            disk_usage: 0.0,
            load_average: 0.0,
            uptime: 0,
            timestamp: chrono::Utc::now(),
        };

        Ok(Self {
            config,
            metrics: Arc::new(RwLock::new(initial_metrics)),
            exchange_metrics: Arc::new(RwLock::new(HashMap::new())),
            alerts: Arc::new(RwLock::new(Vec::new())),
            start_time: Instant::now(),
        })
    }

    pub async fn start_metrics_collection(&self) -> Result<()> {
        let metrics = self.metrics.clone();
        let alerts = self.alerts.clone();
        let interval_duration = Duration::from_secs(self.config.metrics_interval);

        tokio::spawn(async move {
            let mut interval = interval(interval_duration);
            loop {
                interval.tick().await;
                
                if let Err(e) = Self::collect_system_metrics(&metrics, &alerts).await {
                    error!("Failed to collect metrics: {}", e);
                }
            }
        });

        info!("Started metrics collection with {}s interval", self.config.metrics_interval);
        Ok(())
    }

    pub async fn start_metrics_server(&self, port: u16, prometheus: bool) -> Result<()> {
        info!("Starting metrics server on port {} (prometheus: {})", port, prometheus);
        
        // TODO: Start actual metrics server
        // For now, just start the collection
        self.start_metrics_collection().await?;
        
        Ok(())
    }

    async fn collect_system_metrics(
        metrics: &Arc<RwLock<SystemMetrics>>,
        alerts: &Arc<RwLock<Vec<Alert>>>,
    ) -> Result<()> {
        let new_metrics = Self::gather_system_metrics().await?;
        
        // Check for alerts
        let mut alerts_guard = alerts.write().await;
        
        // High CPU usage alert
        if new_metrics.cpu_usage > 80.0 {
            let alert = Alert {
                id: uuid::Uuid::new_v4().to_string(),
                level: if new_metrics.cpu_usage > 90.0 { AlertLevel::Critical } else { AlertLevel::Warning },
                title: "High CPU Usage".to_string(),
                description: format!("CPU usage is at {:.1}%", new_metrics.cpu_usage),
                source: "system".to_string(),
                timestamp: chrono::Utc::now(),
                resolved: false,
            };
            alerts_guard.push(alert);
        }

        // High memory usage alert
        if new_metrics.memory_usage > 85.0 {
            let alert = Alert {
                id: uuid::Uuid::new_v4().to_string(),
                level: if new_metrics.memory_usage > 95.0 { AlertLevel::Critical } else { AlertLevel::Warning },
                title: "High Memory Usage".to_string(),
                description: format!("Memory usage is at {:.1}%", new_metrics.memory_usage),
                source: "system".to_string(),
                timestamp: chrono::Utc::now(),
                resolved: false,
            };
            alerts_guard.push(alert);
        }

        // Update metrics
        *metrics.write().await = new_metrics;
        
        Ok(())
    }

    async fn gather_system_metrics() -> Result<SystemMetrics> {
        // In a real implementation, this would use system APIs to get actual metrics
        // For now, we'll simulate realistic values
        
        use rand::Rng;
        let mut rng = rand::thread_rng();
        
        Ok(SystemMetrics {
            cpu_usage: rng.gen_range(20.0..80.0),
            memory_usage: rng.gen_range(40.0..90.0),
            network_usage: NetworkMetrics {
                inbound_bytes_per_sec: rng.gen_range(1000.0..50000.0),
                outbound_bytes_per_sec: rng.gen_range(500.0..25000.0),
                connections_count: rng.gen_range(10..100),
            },
            disk_usage: rng.gen_range(30.0..80.0),
            load_average: rng.gen_range(0.5..4.0),
            uptime: 3600, // TODO: Calculate actual uptime
            timestamp: chrono::Utc::now(),
        })
    }

    pub async fn get_system_metrics(&self) -> Result<SystemMetrics> {
        Ok(self.metrics.read().await.clone())
    }

    pub async fn get_system_diagnostics(&self) -> Result<SystemDiagnostics> {
        let system_metrics = self.get_system_metrics().await?;
        let exchange_metrics = self.exchange_metrics.read().await.values().cloned().collect();
        let alerts = self.alerts.read().await.clone();

        let sensor_diagnostics = SensorDiagnostics {
            instance_id: uuid::Uuid::new_v4().to_string(), // TODO: Use actual instance ID
            instance_count: 25, // TODO: Get actual instance count
            total_pairs_monitored: 1875, // TODO: Get actual count
            pairs_per_instance_avg: 75.0,
            coverage_percentage: 99.2,
            new_pairs_detected_24h: 5,
            failed_connections: 0,
            memory_usage_per_pair: 0.67, // MB
        };

        Ok(SystemDiagnostics {
            system: system_metrics,
            exchanges: exchange_metrics,
            sensor_specific: sensor_diagnostics,
            alerts,
            generated_at: chrono::Utc::now(),
        })
    }

    pub async fn record_exchange_metrics(&self, exchange: &str, metrics: ExchangeMetrics) {
        self.exchange_metrics.write().await.insert(exchange.to_string(), metrics);
    }

    pub async fn record_data_point(&self, exchange: &str, data_type: &str, latency: f64) {
        // Update exchange-specific metrics
        let mut exchange_metrics = self.exchange_metrics.write().await;
        
        if let Some(metrics) = exchange_metrics.get_mut(exchange) {
            // Update latency metrics (simplified)
            metrics.latency.average = (metrics.latency.average + latency) / 2.0;
            if latency < metrics.latency.min || metrics.latency.min == 0.0 {
                metrics.latency.min = latency;
            }
            if latency > metrics.latency.max {
                metrics.latency.max = latency;
            }
            metrics.last_update = chrono::Utc::now();
        } else {
            // Create new metrics entry
            let new_metrics = ExchangeMetrics {
                exchange: exchange.to_string(),
                connection_status: "connected".to_string(),
                latency: LatencyMetrics {
                    p50: latency,
                    p95: latency,
                    p99: latency,
                    average: latency,
                    min: latency,
                    max: latency,
                },
                throughput: ThroughputMetrics {
                    messages_per_second: 100.0,
                    orders_per_second: 10.0,
                    trades_per_second: 5.0,
                    websocket_messages_per_second: 150.0,
                },
                errors: ErrorMetrics {
                    total_errors: 0,
                    error_rate: 0.0,
                    errors_by_type: HashMap::new(),
                    last_error_time: None,
                },
                pairs_monitored: 75, // TODO: Get actual count
                last_update: chrono::Utc::now(),
            };
            exchange_metrics.insert(exchange.to_string(), new_metrics);
        }
    }

    pub async fn record_error(&self, exchange: &str, error_type: &str) {
        let mut exchange_metrics = self.exchange_metrics.write().await;
        
        if let Some(metrics) = exchange_metrics.get_mut(exchange) {
            metrics.errors.total_errors += 1;
            *metrics.errors.errors_by_type.entry(error_type.to_string()).or_insert(0) += 1;
            metrics.errors.last_error_time = Some(chrono::Utc::now());
        }

        // Create alert for errors
        let alert = Alert {
            id: uuid::Uuid::new_v4().to_string(),
            level: AlertLevel::Warning,
            title: format!("Error in {}", exchange),
            description: format!("Error type: {}", error_type),
            source: exchange.to_string(),
            timestamp: chrono::Utc::now(),
            resolved: false,
        };

        self.alerts.write().await.push(alert);
    }

    pub async fn record_throughput(&self, exchange: &str, messages_per_sec: f64) {
        let mut exchange_metrics = self.exchange_metrics.write().await;
        
        if let Some(metrics) = exchange_metrics.get_mut(exchange) {
            metrics.throughput.messages_per_second = messages_per_sec;
            metrics.last_update = chrono::Utc::now();
        }
    }

    pub async fn get_connection_status(&self, exchange: &str) -> String {
        if let Some(metrics) = self.exchange_metrics.read().await.get(exchange) {
            metrics.connection_status.clone()
        } else {
            "unknown".to_string()
        }
    }

    pub async fn get_latency_metrics(&self, exchange: &str) -> Option<LatencyMetrics> {
        self.exchange_metrics.read().await.get(exchange).map(|m| m.latency.clone())
    }

    pub async fn get_throughput_metrics(&self, exchange: &str) -> Option<ThroughputMetrics> {
        self.exchange_metrics.read().await.get(exchange).map(|m| m.throughput.clone())
    }

    pub async fn get_error_metrics(&self, exchange: &str) -> Option<ErrorMetrics> {
        self.exchange_metrics.read().await.get(exchange).map(|m| m.errors.clone())
    }

    pub async fn check_circuit_breaker(&self, exchange: &str, threshold: usize) -> bool {
        if let Some(metrics) = self.exchange_metrics.read().await.get(exchange) {
            metrics.errors.total_errors as usize >= threshold
        } else {
            false
        }
    }

    pub async fn get_active_alerts(&self) -> Vec<Alert> {
        self.alerts.read().await
            .iter()
            .filter(|alert| !alert.resolved)
            .cloned()
            .collect()
    }

    pub async fn resolve_alert(&self, alert_id: &str) {
        let mut alerts = self.alerts.write().await;
        if let Some(alert) = alerts.iter_mut().find(|a| a.id == alert_id) {
            alert.resolved = true;
        }
    }

    pub async fn clear_old_alerts(&self, max_age_hours: i64) {
        let cutoff = chrono::Utc::now() - chrono::Duration::hours(max_age_hours);
        let mut alerts = self.alerts.write().await;
        alerts.retain(|alert| alert.timestamp > cutoff);
    }

    pub fn get_uptime(&self) -> Duration {
        self.start_time.elapsed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MonitoringConfig;

    #[tokio::test]
    async fn test_health_monitor_creation() {
        let config = MonitoringConfig {
            prometheus_port: 9090,
            health_port: 8080,
            metrics_interval: 60,
            datadog_api_key: None,
            sentry_dsn: None,
            log_level: "info".to_string(),
        };

        let monitor = HealthMonitor::new(config).await.unwrap();
        assert!(monitor.get_uptime().as_secs() < 1);
    }

    #[tokio::test]
    async fn test_metrics_collection() {
        let config = MonitoringConfig {
            prometheus_port: 9090,
            health_port: 8080,
            metrics_interval: 1, // 1 second for testing
            datadog_api_key: None,
            sentry_dsn: None,
            log_level: "info".to_string(),
        };

        let monitor = HealthMonitor::new(config).await.unwrap();
        
        // Record some test data
        monitor.record_data_point("binance", "ticker", 15.5).await;
        monitor.record_error("binance", "connection_timeout").await;
        monitor.record_throughput("binance", 1500.0).await;

        let metrics = monitor.get_latency_metrics("binance").await;
        assert!(metrics.is_some());

        let alerts = monitor.get_active_alerts().await;
        assert!(!alerts.is_empty());
    }

    #[tokio::test]
    async fn test_alert_management() {
        let config = MonitoringConfig {
            prometheus_port: 9090,
            health_port: 8080,
            metrics_interval: 60,
            datadog_api_key: None,
            sentry_dsn: None,
            log_level: "info".to_string(),
        };

        let monitor = HealthMonitor::new(config).await.unwrap();
        
        // Create some errors to generate alerts
        monitor.record_error("binance", "rate_limit").await;
        monitor.record_error("coinbase", "connection_error").await;

        let alerts = monitor.get_active_alerts().await;
        assert_eq!(alerts.len(), 2);

        // Resolve an alert
        if let Some(alert_id) = alerts.first().map(|a| a.id.clone()) {
            monitor.resolve_alert(&alert_id).await;
        }

        let active_alerts = monitor.get_active_alerts().await;
        assert_eq!(active_alerts.len(), 1);
    }
}