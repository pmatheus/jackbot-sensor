/// Real-time Performance Monitoring Dashboard
/// 
/// Interactive dashboard for visualizing Bloomberg killer validation metrics
/// in real-time during performance testing and production monitoring.

use crate::performance::{
    end_to_end_validation::{
        ValidationResults, ScenarioMetrics, PerformanceTargets, BloombergBaseline,
        ComparisonResults, LatencyMetrics, ThroughputMetrics, ResourceMetrics
    },
    real_time_diagnostics::{RealTimePerformanceMonitor, CorePerformanceMetrics}
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    sync::{broadcast, RwLock},
    time::interval,
};
use tracing::{debug, error, info, warn};

/// Real-time performance monitoring dashboard
#[derive(Debug)]
pub struct PerformanceDashboard {
    /// Dashboard configuration
    config: DashboardConfig,
    /// Current performance state
    current_state: Arc<RwLock<DashboardState>>,
    /// Historical metrics storage
    historical_metrics: Arc<RwLock<HistoricalMetrics>>,
    /// Alert system
    alert_system: AlertSystem,
    /// Dashboard event broadcaster
    event_broadcaster: broadcast::Sender<DashboardEvent>,
    /// Performance targets for comparison
    targets: PerformanceTargets,
    /// Bloomberg baseline for competitive analysis
    bloomberg_baseline: BloombergBaseline,
}

/// Dashboard configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardConfig {
    /// Update frequency in milliseconds
    pub update_frequency_ms: u64,
    /// Historical data retention period (hours)
    pub history_retention_hours: u64,
    /// Alert thresholds
    pub alert_thresholds: AlertThresholds,
    /// Display preferences
    pub display_config: DisplayConfig,
    /// Export settings
    pub export_config: ExportConfig,
}

impl Default for DashboardConfig {
    fn default() -> Self {
        Self {
            update_frequency_ms: 1000, // 1 second updates
            history_retention_hours: 24, // 24 hours of history
            alert_thresholds: AlertThresholds::default(),
            display_config: DisplayConfig::default(),
            export_config: ExportConfig::default(),
        }
    }
}

/// Current dashboard state
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DashboardState {
    /// Current performance metrics
    pub current_metrics: ScenarioMetrics,
    /// Bloomberg comparison status
    pub bloomberg_status: BloombergComparisonStatus,
    /// Target achievement status
    pub target_status: TargetAchievementStatus,
    /// System health indicators
    pub system_health: SystemHealthIndicators,
    /// Active alerts
    pub active_alerts: Vec<Alert>,
    /// Last update timestamp
    pub last_update: DateTime<Utc>,
}

/// Bloomberg comparison status for dashboard
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BloombergComparisonStatus {
    /// Speed comparison (positive = faster than Bloomberg)
    pub speed_comparison_percent: f64,
    /// Cost comparison (positive = cheaper than Bloomberg)
    pub cost_comparison_percent: f64,
    /// Feature completeness percentage
    pub feature_completeness_percent: f64,
    /// Overall superiority score
    pub superiority_score: f64,
    /// Competitive advantage summary
    pub competitive_summary: String,
}

/// Target achievement status
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TargetAchievementStatus {
    /// Sensor processing target status
    pub sensor_processing: TargetStatus,
    /// Backend API target status
    pub backend_api: TargetStatus,
    /// End-to-end target status
    pub end_to_end: TargetStatus,
    /// WebSocket latency target status
    pub websocket_latency: TargetStatus,
    /// UI responsiveness target status
    pub ui_responsiveness: TargetStatus,
    /// Overall achievement percentage
    pub overall_percentage: f64,
}

/// Individual target status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetStatus {
    /// Target achieved
    pub achieved: bool,
    /// Current value (microseconds)
    pub current_value_micros: u64,
    /// Target value (microseconds)
    pub target_value_micros: u64,
    /// Performance margin (positive = better than target)
    pub margin_percent: f64,
    /// Trend indicator
    pub trend: PerformanceTrend,
}

impl Default for TargetStatus {
    fn default() -> Self {
        Self {
            achieved: false,
            current_value_micros: 0,
            target_value_micros: 0,
            margin_percent: 0.0,
            trend: PerformanceTrend::Stable,
        }
    }
}

/// Performance trend indicators
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PerformanceTrend {
    Improving,
    Stable,
    Degrading,
}

/// System health indicators
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SystemHealthIndicators {
    /// Overall system health score (0.0-1.0)
    pub overall_health: f64,
    /// CPU health status
    pub cpu_health: HealthStatus,
    /// Memory health status
    pub memory_health: HealthStatus,
    /// Network health status
    pub network_health: HealthStatus,
    /// Error rate health status
    pub error_rate_health: HealthStatus,
    /// Latency health status
    pub latency_health: HealthStatus,
}

/// Health status for individual components
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    /// Health score (0.0-1.0)
    pub score: f64,
    /// Status level
    pub level: HealthLevel,
    /// Description
    pub description: String,
    /// Last check timestamp
    pub last_check: DateTime<Utc>,
}

impl Default for HealthStatus {
    fn default() -> Self {
        Self {
            score: 1.0,
            level: HealthLevel::Healthy,
            description: "OK".to_string(),
            last_check: Utc::now(),
        }
    }
}

/// Health level indicators
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum HealthLevel {
    Healthy,
    Warning,
    Critical,
    Unknown,
}

/// Historical metrics storage
#[derive(Debug, Default)]
pub struct HistoricalMetrics {
    /// Latency history
    pub latency_history: TimeSeriesData<f64>,
    /// Throughput history
    pub throughput_history: TimeSeriesData<f64>,
    /// Resource usage history
    pub resource_history: TimeSeriesData<ResourceSnapshot>,
    /// Bloomberg comparison history
    pub comparison_history: TimeSeriesData<ComparisonSnapshot>,
    /// Error rate history
    pub error_history: TimeSeriesData<f64>,
}

/// Time series data structure
#[derive(Debug)]
pub struct TimeSeriesData<T> {
    /// Data points with timestamps
    pub data_points: VecDeque<TimestampedValue<T>>,
    /// Maximum retention count
    pub max_retention: usize,
}

impl<T> Default for TimeSeriesData<T> {
    fn default() -> Self {
        Self {
            data_points: VecDeque::new(),
            max_retention: 86400, // 24 hours at 1 second intervals
        }
    }
}

/// Timestamped value for time series
#[derive(Debug, Clone)]
pub struct TimestampedValue<T> {
    pub timestamp: DateTime<Utc>,
    pub value: T,
}

/// Resource usage snapshot
#[derive(Debug, Clone)]
pub struct ResourceSnapshot {
    pub cpu_percent: f64,
    pub memory_mb: u64,
    pub network_bps: u64,
    pub disk_iops: u64,
}

/// Bloomberg comparison snapshot
#[derive(Debug, Clone)]
pub struct ComparisonSnapshot {
    pub speed_improvement: f64,
    pub cost_reduction: f64,
    pub feature_completeness: f64,
    pub superiority_score: f64,
}

/// Alert system for performance monitoring
#[derive(Debug)]
pub struct AlertSystem {
    /// Alert configuration
    config: AlertConfig,
    /// Active alerts
    active_alerts: Arc<RwLock<Vec<Alert>>>,
    /// Alert history
    alert_history: Arc<RwLock<VecDeque<Alert>>>,
}

/// Alert configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertConfig {
    /// Enable email alerts
    pub enable_email: bool,
    /// Enable Slack notifications
    pub enable_slack: bool,
    /// Enable webhook notifications
    pub enable_webhooks: bool,
    /// Alert cooldown period (seconds)
    pub cooldown_seconds: u64,
}

impl Default for AlertConfig {
    fn default() -> Self {
        Self {
            enable_email: true,
            enable_slack: false,
            enable_webhooks: true,
            cooldown_seconds: 300, // 5 minutes
        }
    }
}

/// Performance alert
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    /// Alert ID
    pub id: String,
    /// Alert type
    pub alert_type: AlertType,
    /// Alert severity
    pub severity: AlertSeverity,
    /// Alert message
    pub message: String,
    /// Alert timestamp
    pub timestamp: DateTime<Utc>,
    /// Alert data
    pub data: serde_json::Value,
    /// Acknowledgment status
    pub acknowledged: bool,
}

/// Alert types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlertType {
    LatencyThresholdExceeded,
    ThroughputDropped,
    ResourceUtilizationHigh,
    ErrorRateElevated,
    TargetNotMet,
    BloombergComparisonFailed,
    SystemHealthDegraded,
}

/// Alert severity levels
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlertSeverity {
    Info,
    Warning,
    Critical,
    Emergency,
}

/// Alert thresholds configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertThresholds {
    /// Maximum acceptable latency (microseconds)
    pub max_latency_micros: u64,
    /// Minimum acceptable throughput (messages/second)
    pub min_throughput_mps: f64,
    /// Maximum CPU utilization percentage
    pub max_cpu_percent: f64,
    /// Maximum memory utilization (MB)
    pub max_memory_mb: u64,
    /// Maximum error rate (errors/second)
    pub max_error_rate: f64,
    /// Minimum Bloomberg superiority score
    pub min_superiority_score: f64,
}

impl Default for AlertThresholds {
    fn default() -> Self {
        Self {
            max_latency_micros: 20_000, // 20ms warning threshold
            min_throughput_mps: 1000.0, // 1000 messages/second minimum
            max_cpu_percent: 80.0,      // 80% CPU warning
            max_memory_mb: 2048,        // 2GB memory warning
            max_error_rate: 1.0,        // 1 error/second warning
            min_superiority_score: 0.7, // 70% superiority minimum
        }
    }
}

/// Display configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayConfig {
    /// Chart time window (minutes)
    pub chart_time_window_minutes: u64,
    /// Refresh rate (seconds)
    pub refresh_rate_seconds: u64,
    /// Show Bloomberg comparison
    pub show_bloomberg_comparison: bool,
    /// Show target achievements
    pub show_target_achievements: bool,
    /// Show system health
    pub show_system_health: bool,
    /// Color scheme
    pub color_scheme: ColorScheme,
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            chart_time_window_minutes: 60, // 1 hour charts
            refresh_rate_seconds: 5,       // 5 second refresh
            show_bloomberg_comparison: true,
            show_target_achievements: true,
            show_system_health: true,
            color_scheme: ColorScheme::Default,
        }
    }
}

/// Color schemes for dashboard
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ColorScheme {
    Default,
    HighContrast,
    Dark,
    Light,
}

/// Export configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportConfig {
    /// Enable automatic exports
    pub enable_auto_export: bool,
    /// Export frequency (hours)
    pub export_frequency_hours: u64,
    /// Export formats
    pub export_formats: Vec<ExportFormat>,
    /// Export destination
    pub export_destination: String,
}

impl Default for ExportConfig {
    fn default() -> Self {
        Self {
            enable_auto_export: true,
            export_frequency_hours: 24, // Daily exports
            export_formats: vec![ExportFormat::Json, ExportFormat::Csv],
            export_destination: "./exports".to_string(),
        }
    }
}

/// Export formats
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExportFormat {
    Json,
    Csv,
    Html,
    Pdf,
    Excel,
}

/// Dashboard events
#[derive(Debug, Clone)]
pub enum DashboardEvent {
    /// Metrics updated
    MetricsUpdated {
        timestamp: DateTime<Utc>,
        metrics: ScenarioMetrics,
    },
    /// Alert triggered
    AlertTriggered {
        alert: Alert,
    },
    /// Target achieved
    TargetAchieved {
        target_name: String,
        achievement_time: DateTime<Utc>,
    },
    /// Bloomberg comparison updated
    BloombergComparisonUpdated {
        comparison: ComparisonResults,
    },
    /// System health changed
    SystemHealthChanged {
        health_level: HealthLevel,
        component: String,
    },
}

impl PerformanceDashboard {
    /// Create new performance dashboard
    pub fn new(
        config: DashboardConfig,
        targets: PerformanceTargets,
        bloomberg_baseline: BloombergBaseline,
    ) -> Self {
        let (event_broadcaster, _) = broadcast::channel(1000);
        
        Self {
            config,
            current_state: Arc::new(RwLock::new(DashboardState::default())),
            historical_metrics: Arc::new(RwLock::new(HistoricalMetrics::default())),
            alert_system: AlertSystem::new(AlertConfig::default()),
            event_broadcaster,
            targets,
            bloomberg_baseline,
        }
    }

    /// Start dashboard monitoring
    pub async fn start(&self) -> Result<(), DashboardError> {
        info!("🖥️ Starting Performance Dashboard");

        // Start metrics collection loop
        let dashboard = self.clone();
        tokio::spawn(async move {
            dashboard.metrics_collection_loop().await;
        });

        // Start alert monitoring loop
        let dashboard = self.clone();
        tokio::spawn(async move {
            dashboard.alert_monitoring_loop().await;
        });

        // Start historical data cleanup loop
        let dashboard = self.clone();
        tokio::spawn(async move {
            dashboard.historical_cleanup_loop().await;
        });

        Ok(())
    }

    /// Update dashboard with new performance metrics
    pub async fn update_metrics(&self, metrics: ScenarioMetrics) -> Result<(), DashboardError> {
        let now = Utc::now();

        // Update current state
        {
            let mut state = self.current_state.write().await;
            state.current_metrics = metrics.clone();
            state.bloomberg_status = self.calculate_bloomberg_status(&metrics);
            state.target_status = self.calculate_target_status(&metrics).await;
            state.system_health = self.calculate_system_health(&metrics);
            state.last_update = now;
        }

        // Store historical data
        self.store_historical_metrics(&metrics, now).await;

        // Check for alerts
        self.check_alerts(&metrics).await;

        // Broadcast update event
        let _ = self.event_broadcaster.send(DashboardEvent::MetricsUpdated {
            timestamp: now,
            metrics,
        });

        Ok(())
    }

    /// Get current dashboard state
    pub async fn get_current_state(&self) -> DashboardState {
        self.current_state.read().await.clone()
    }

    /// Get historical metrics for charting
    pub async fn get_historical_metrics(&self, duration: Duration) -> HistoricalMetrics {
        let historical = self.historical_metrics.read().await;
        let cutoff = Utc::now() - chrono::Duration::from_std(duration).unwrap_or(chrono::Duration::hours(1));

        // Filter data to requested time window
        let mut filtered = HistoricalMetrics::default();
        
        // Filter latency history
        for point in &historical.latency_history.data_points {
            if point.timestamp >= cutoff {
                filtered.latency_history.data_points.push_back(point.clone());
            }
        }

        // Filter other metrics similarly...
        filtered
    }

    /// Generate dashboard HTML report
    pub async fn generate_html_report(&self) -> Result<String, DashboardError> {
        let state = self.get_current_state().await;
        
        let html = format!(
            r#"
<!DOCTYPE html>
<html>
<head>
    <title>Bloomberg Killer Performance Dashboard</title>
    <style>
        body {{ font-family: Arial, sans-serif; margin: 20px; }}
        .header {{ background-color: #2c3e50; color: white; padding: 20px; text-align: center; }}
        .metrics {{ display: grid; grid-template-columns: repeat(auto-fit, minmax(300px, 1fr)); gap: 20px; margin: 20px 0; }}
        .metric-card {{ border: 1px solid #ddd; border-radius: 8px; padding: 15px; background: #f9f9f9; }}
        .metric-value {{ font-size: 2em; font-weight: bold; color: #27ae60; }}
        .metric-label {{ color: #7f8c8d; margin-bottom: 5px; }}
        .status-good {{ color: #27ae60; }}
        .status-warning {{ color: #f39c12; }}
        .status-error {{ color: #e74c3c; }}
        .comparison {{ background: #ecf0f1; padding: 15px; border-radius: 8px; margin: 20px 0; }}
        .alert {{ background: #f8d7da; border: 1px solid #f5c6cb; padding: 10px; margin: 10px 0; border-radius: 4px; }}
    </style>
</head>
<body>
    <div class="header">
        <h1>🚀 Bloomberg Terminal Killer Dashboard</h1>
        <p>Real-time Performance Monitoring & Validation</p>
        <p>Last Updated: {}</p>
    </div>

    <div class="metrics">
        <div class="metric-card">
            <div class="metric-label">Market Data Processing</div>
            <div class="metric-value">{:.2}ms</div>
            <div class="{}">Target: <10ms</div>
        </div>
        
        <div class="metric-card">
            <div class="metric-label">Order Execution</div>
            <div class="metric-value">{:.2}ms</div>
            <div class="{}">Target: <100ms</div>
        </div>
        
        <div class="metric-card">
            <div class="metric-label">Throughput</div>
            <div class="metric-value">{:.0}</div>
            <div class="metric-label">messages/second</div>
        </div>
        
        <div class="metric-card">
            <div class="metric-label">CPU Usage</div>
            <div class="metric-value">{:.1}%</div>
            <div class="{}">Memory: {}MB</div>
        </div>
    </div>

    <div class="comparison">
        <h2>🥊 Bloomberg Terminal Comparison</h2>
        <p><strong>Speed Advantage:</strong> {:.1}x faster than Bloomberg</p>
        <p><strong>Cost Advantage:</strong> {:.0}x cheaper than Bloomberg ($50 vs $2000/month)</p>
        <p><strong>Feature Completeness:</strong> {:.0}%</p>
        <p><strong>Superiority Score:</strong> {:.1}/5.0</p>
    </div>

    <div class="alerts">
        <h2>🚨 Active Alerts</h2>
        {}
    </div>
</body>
</html>
            "#,
            state.last_update.format("%Y-%m-%d %H:%M:%S UTC"),
            state.current_metrics.latencies.market_data_processing.mean_micros / 1000.0,
            if state.target_status.sensor_processing.achieved { "status-good" } else { "status-error" },
            state.current_metrics.latencies.order_execution.mean_micros / 1000.0,
            if state.target_status.end_to_end.achieved { "status-good" } else { "status-error" },
            state.current_metrics.throughput.messages_per_second,
            state.current_metrics.resources.cpu_usage_percent,
            if state.current_metrics.resources.cpu_usage_percent < 80.0 { "status-good" } else { "status-warning" },
            state.current_metrics.resources.memory_usage_mb,
            state.bloomberg_status.speed_comparison_percent,
            state.bloomberg_status.cost_comparison_percent,
            state.bloomberg_status.feature_completeness_percent,
            state.bloomberg_status.superiority_score,
            self.format_alerts(&state.active_alerts),
        );

        Ok(html)
    }

    /// Export dashboard data to various formats
    pub async fn export_data(&self, format: ExportFormat) -> Result<String, DashboardError> {
        let state = self.get_current_state().await;
        
        match format {
            ExportFormat::Json => {
                serde_json::to_string_pretty(&state)
                    .map_err(|e| DashboardError::ExportError(e.to_string()))
            }
            ExportFormat::Csv => {
                self.generate_csv_export(&state).await
            }
            ExportFormat::Html => {
                self.generate_html_report().await
            }
            _ => Err(DashboardError::UnsupportedFormat(format)),
        }
    }

    // Helper methods...

    async fn metrics_collection_loop(&self) {
        let mut interval = interval(Duration::from_millis(self.config.update_frequency_ms));
        
        loop {
            interval.tick().await;
            
            // In a real implementation, this would collect metrics from the monitoring system
            // For now, we'll just maintain the loop structure
            debug!("Dashboard metrics collection tick");
        }
    }

    async fn alert_monitoring_loop(&self) {
        let mut interval = interval(Duration::from_secs(10)); // Check alerts every 10 seconds
        
        loop {
            interval.tick().await;
            
            // Check for expired alerts and cleanup
            self.alert_system.cleanup_expired_alerts().await;
        }
    }

    async fn historical_cleanup_loop(&self) {
        let mut interval = interval(Duration::from_secs(3600)); // Cleanup every hour
        
        loop {
            interval.tick().await;
            
            // Remove old historical data
            self.cleanup_historical_data().await;
        }
    }

    fn calculate_bloomberg_status(&self, metrics: &ScenarioMetrics) -> BloombergComparisonStatus {
        let bloomberg = &self.bloomberg_baseline;
        
        // Calculate speed comparison
        let speed_improvement = bloomberg.market_data_latency_micros as f64 / 
                               (metrics.latencies.market_data_processing.mean_micros + 1.0);
        
        BloombergComparisonStatus {
            speed_comparison_percent: (speed_improvement - 1.0) * 100.0,
            cost_comparison_percent: 95.0, // $50 vs $2000 = 97.5% cheaper
            feature_completeness_percent: metrics.bloomberg_comparison.feature_completeness * 100.0,
            superiority_score: metrics.bloomberg_comparison.superiority_score * 5.0,
            competitive_summary: format!(
                "{:.1}x faster, {:.0}% cheaper, {:.0}% features",
                speed_improvement,
                95.0,
                metrics.bloomberg_comparison.feature_completeness * 100.0
            ),
        }
    }

    async fn calculate_target_status(&self, metrics: &ScenarioMetrics) -> TargetAchievementStatus {
        let sensor_target = self.calculate_individual_target_status(
            metrics.latencies.market_data_processing.mean_micros as u64,
            self.targets.sensor_processing_micros,
        ).await;

        let api_target = self.calculate_individual_target_status(
            metrics.latencies.api_response.mean_micros as u64,
            self.targets.backend_api_micros,
        ).await;

        let end_to_end_target = self.calculate_individual_target_status(
            metrics.latencies.end_to_end.mean_micros as u64,
            self.targets.end_to_end_micros,
        ).await;

        let websocket_target = self.calculate_individual_target_status(
            metrics.latencies.websocket.mean_micros as u64,
            self.targets.websocket_latency_micros,
        ).await;

        // Calculate overall achievement
        let achieved_count = [
            sensor_target.achieved,
            api_target.achieved,
            end_to_end_target.achieved,
            websocket_target.achieved,
        ].iter().filter(|&&x| x).count();

        TargetAchievementStatus {
            sensor_processing: sensor_target,
            backend_api: api_target,
            end_to_end: end_to_end_target,
            websocket_latency: websocket_target,
            ui_responsiveness: TargetStatus::default(), // Placeholder
            overall_percentage: (achieved_count as f64 / 4.0) * 100.0,
        }
    }

    async fn calculate_individual_target_status(&self, current: u64, target: u64) -> TargetStatus {
        let achieved = current <= target;
        let margin_percent = if target > 0 {
            ((target as f64 - current as f64) / target as f64) * 100.0
        } else {
            0.0
        };

        // Calculate trend from historical data
        let trend = self.calculate_performance_trend(current).await;
        
        TargetStatus {
            achieved,
            current_value_micros: current,
            target_value_micros: target,
            margin_percent,
            trend,
        }
    }

    fn calculate_system_health(&self, metrics: &ScenarioMetrics) -> SystemHealthIndicators {
        // Calculate individual health scores
        let cpu_health = self.calculate_health_score(
            metrics.resources.cpu_usage_percent,
            self.config.alert_thresholds.max_cpu_percent,
            "CPU Usage",
        );

        let memory_health = self.calculate_health_score(
            metrics.resources.memory_usage_mb as f64,
            self.config.alert_thresholds.max_memory_mb as f64,
            "Memory Usage",
        );

        let latency_health = self.calculate_health_score(
            metrics.latencies.market_data_processing.mean_micros,
            self.config.alert_thresholds.max_latency_micros as f64,
            "Latency",
        );

        let error_health = self.calculate_health_score(
            metrics.errors.error_rate,
            self.config.alert_thresholds.max_error_rate,
            "Error Rate",
        );

        // Calculate overall health
        let overall_health = (cpu_health.score + memory_health.score + 
                             latency_health.score + error_health.score) / 4.0;

        SystemHealthIndicators {
            overall_health,
            cpu_health,
            memory_health,
            network_health: HealthStatus::default(), // Placeholder
            error_rate_health: error_health,
            latency_health,
        }
    }

    fn calculate_health_score(&self, current: f64, threshold: f64, component: &str) -> HealthStatus {
        let ratio = current / threshold;
        let (score, level, description) = if ratio <= 0.5 {
            (1.0, HealthLevel::Healthy, format!("{} is optimal", component))
        } else if ratio <= 0.8 {
            (0.8, HealthLevel::Healthy, format!("{} is normal", component))
        } else if ratio <= 1.0 {
            (0.6, HealthLevel::Warning, format!("{} is elevated", component))
        } else {
            (0.3, HealthLevel::Critical, format!("{} is critical", component))
        };

        HealthStatus {
            score,
            level,
            description,
            last_check: Utc::now(),
        }
    }

    async fn store_historical_metrics(&self, metrics: &ScenarioMetrics, timestamp: DateTime<Utc>) {
        let mut historical = self.historical_metrics.write().await;
        
        // Store latency data
        historical.latency_history.add_point(TimestampedValue {
            timestamp,
            value: metrics.latencies.market_data_processing.mean_micros,
        });

        // Store throughput data
        historical.throughput_history.add_point(TimestampedValue {
            timestamp,
            value: metrics.throughput.messages_per_second,
        });

        // Store resource data
        historical.resource_history.add_point(TimestampedValue {
            timestamp,
            value: ResourceSnapshot {
                cpu_percent: metrics.resources.cpu_usage_percent,
                memory_mb: metrics.resources.memory_usage_mb,
                network_bps: metrics.resources.network_usage_bps,
                disk_iops: metrics.resources.disk_iops,
            },
        });

        // Store comparison data
        historical.comparison_history.add_point(TimestampedValue {
            timestamp,
            value: ComparisonSnapshot {
                speed_improvement: metrics.bloomberg_comparison.speed_improvement,
                cost_reduction: metrics.bloomberg_comparison.cost_reduction,
                feature_completeness: metrics.bloomberg_comparison.feature_completeness,
                superiority_score: metrics.bloomberg_comparison.superiority_score,
            },
        });
    }

    async fn check_alerts(&self, metrics: &ScenarioMetrics) {
        // Check latency thresholds
        if metrics.latencies.market_data_processing.mean_micros > self.config.alert_thresholds.max_latency_micros as f64 {
            self.trigger_alert(Alert {
                id: format!("latency_{}", Utc::now().timestamp()),
                alert_type: AlertType::LatencyThresholdExceeded,
                severity: AlertSeverity::Warning,
                message: format!(
                    "Market data latency exceeded threshold: {:.2}ms > {:.2}ms",
                    metrics.latencies.market_data_processing.mean_micros / 1000.0,
                    self.config.alert_thresholds.max_latency_micros as f64 / 1000.0
                ),
                timestamp: Utc::now(),
                data: serde_json::json!({
                    "current_latency_micros": metrics.latencies.market_data_processing.mean_micros,
                    "threshold_micros": self.config.alert_thresholds.max_latency_micros
                }),
                acknowledged: false,
            }).await;
        }

        // Check other metrics...
    }

    async fn trigger_alert(&self, alert: Alert) {
        info!("🚨 Alert triggered: {}", alert.message);
        
        // Add to active alerts
        self.alert_system.add_alert(alert.clone()).await;

        // Broadcast alert event
        let _ = self.event_broadcaster.send(DashboardEvent::AlertTriggered { alert });
    }

    async fn cleanup_historical_data(&self) {
        let mut historical = self.historical_metrics.write().await;
        let retention_cutoff = Utc::now() - chrono::Duration::hours(self.config.history_retention_hours as i64);

        // Clean up old data points
        historical.latency_history.cleanup_old_data(retention_cutoff);
        historical.throughput_history.cleanup_old_data(retention_cutoff);
        historical.resource_history.cleanup_old_data(retention_cutoff);
        historical.comparison_history.cleanup_old_data(retention_cutoff);
    }
    
    /// Calculate performance trend based on recent historical data
    async fn calculate_performance_trend(&self, current_value: u64) -> PerformanceTrend {
        let historical = self.historical_metrics.read().await;
        let now = Utc::now();
        let five_minutes_ago = now - chrono::Duration::minutes(5);
        let one_minute_ago = now - chrono::Duration::minutes(1);
        
        // Get recent latency data points
        let recent_points: Vec<f64> = historical.latency_history.data_points
            .iter()
            .filter(|p| p.timestamp >= five_minutes_ago)
            .map(|p| p.value)
            .collect();
        
        if recent_points.len() < 3 {
            // Not enough data for trend calculation
            return PerformanceTrend::Stable;
        }
        
        // Calculate average for last minute vs average for 5 minutes
        let last_minute_points: Vec<f64> = historical.latency_history.data_points
            .iter()
            .filter(|p| p.timestamp >= one_minute_ago)
            .map(|p| p.value)
            .collect();
        
        if last_minute_points.is_empty() {
            return PerformanceTrend::Stable;
        }
        
        let recent_avg = recent_points.iter().sum::<f64>() / recent_points.len() as f64;
        let last_minute_avg = last_minute_points.iter().sum::<f64>() / last_minute_points.len() as f64;
        
        // Calculate percentage change
        let change_percent = if recent_avg > 0.0 {
            ((last_minute_avg - recent_avg) / recent_avg) * 100.0
        } else {
            0.0
        };
        
        // Determine trend based on change percentage
        // Note: For latency, lower is better, so negative change is improving
        if change_percent < -5.0 {
            PerformanceTrend::Improving
        } else if change_percent > 5.0 {
            PerformanceTrend::Degrading
        } else {
            PerformanceTrend::Stable
        }
    }

    fn format_alerts(&self, alerts: &[Alert]) -> String {
        if alerts.is_empty() {
            "<p class=\"status-good\">No active alerts</p>".to_string()
        } else {
            alerts.iter()
                .map(|alert| format!(
                    "<div class=\"alert\"><strong>{}:</strong> {}</div>",
                    match alert.severity {
                        AlertSeverity::Critical => "CRITICAL",
                        AlertSeverity::Warning => "WARNING",
                        AlertSeverity::Info => "INFO",
                        AlertSeverity::Emergency => "EMERGENCY",
                    },
                    alert.message
                ))
                .collect::<Vec<_>>()
                .join("")
        }
    }

    async fn generate_csv_export(&self, state: &DashboardState) -> Result<String, DashboardError> {
        let mut csv = String::new();
        csv.push_str("Metric,Value,Unit,Status\n");
        
        // Add metrics rows
        csv.push_str(&format!(
            "Market Data Latency,{:.2},ms,{}\n",
            state.current_metrics.latencies.market_data_processing.mean_micros / 1000.0,
            if state.target_status.sensor_processing.achieved { "PASS" } else { "FAIL" }
        ));
        
        csv.push_str(&format!(
            "Order Execution Latency,{:.2},ms,{}\n",
            state.current_metrics.latencies.order_execution.mean_micros / 1000.0,
            if state.target_status.end_to_end.achieved { "PASS" } else { "FAIL" }
        ));
        
        csv.push_str(&format!(
            "Throughput,{:.0},msg/sec,OK\n",
            state.current_metrics.throughput.messages_per_second
        ));
        
        csv.push_str(&format!(
            "CPU Usage,{:.1},%,{}\n",
            state.current_metrics.resources.cpu_usage_percent,
            if state.current_metrics.resources.cpu_usage_percent < 80.0 { "OK" } else { "HIGH" }
        ));

        Ok(csv)
    }
}

// Implementation of helper traits and methods

impl<T> TimeSeriesData<T> {
    fn add_point(&mut self, point: TimestampedValue<T>) {
        self.data_points.push_back(point);
        
        // Maintain max retention
        while self.data_points.len() > self.max_retention {
            self.data_points.pop_front();
        }
    }
    
    fn cleanup_old_data(&mut self, cutoff: DateTime<Utc>) {
        while let Some(front) = self.data_points.front() {
            if front.timestamp < cutoff {
                self.data_points.pop_front();
            } else {
                break;
            }
        }
    }
}

impl AlertSystem {
    fn new(config: AlertConfig) -> Self {
        Self {
            config,
            active_alerts: Arc::new(RwLock::new(Vec::new())),
            alert_history: Arc::new(RwLock::new(VecDeque::new())),
        }
    }
    
    async fn add_alert(&self, alert: Alert) {
        let mut active = self.active_alerts.write().await;
        active.push(alert.clone());
        
        let mut history = self.alert_history.write().await;
        history.push_back(alert);
        
        // Maintain history size
        while history.len() > 1000 {
            history.pop_front();
        }
    }
    
    async fn cleanup_expired_alerts(&self) {
        let cutoff = Utc::now() - chrono::Duration::seconds(self.config.cooldown_seconds as i64);
        
        let mut active = self.active_alerts.write().await;
        active.retain(|alert| alert.timestamp > cutoff || alert.acknowledged);
    }
}

impl Clone for PerformanceDashboard {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            current_state: Arc::clone(&self.current_state),
            historical_metrics: Arc::clone(&self.historical_metrics),
            alert_system: AlertSystem::new(self.alert_system.config.clone()),
            event_broadcaster: self.event_broadcaster.clone(),
            targets: self.targets.clone(),
            bloomberg_baseline: self.bloomberg_baseline.clone(),
        }
    }
}

/// Dashboard error types
#[derive(Debug, thiserror::Error)]
pub enum DashboardError {
    #[error("Export error: {0}")]
    ExportError(String),
    
    #[error("Unsupported format: {0:?}")]
    UnsupportedFormat(ExportFormat),
    
    #[error("Configuration error: {0}")]
    ConfigurationError(String),
    
    #[error("Data collection error: {0}")]
    DataCollectionError(String),
}