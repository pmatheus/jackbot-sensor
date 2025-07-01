use anyhow::Result;
use serde::{Serialize, Deserialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{interval, Duration};
use tracing::{info, warn, error, debug};

use crate::config::DiscoveryConfig;
use crate::sensor::{NewPairAlert, DetectionMethod, AlertPriority};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingPair {
    pub symbol: String,
    pub base_asset: String,
    pub quote_asset: String,
    pub status: String,
    pub trading_start_time: Option<chrono::DateTime<chrono::Utc>>,
    pub min_quantity: Option<f64>,
    pub max_quantity: Option<f64>,
    pub tick_size: Option<f64>,
    pub min_notional: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExchangeInfo {
    pub exchange: String,
    pub total_pairs: usize,
    pub monitored_pairs: usize,
    pub new_pairs_detected: usize,
    pub last_check: chrono::DateTime<chrono::Utc>,
    pub coverage_percentage: f64,
}

#[derive(Clone)]
pub struct PairDiscovery {
    config: DiscoveryConfig,
    known_pairs: Arc<RwLock<HashMap<String, HashSet<String>>>>, // exchange -> pairs
    exchange_info: Arc<RwLock<HashMap<String, ExchangeInfo>>>,
    last_discovery_run: Arc<RwLock<chrono::DateTime<chrono::Utc>>>,
}

impl PairDiscovery {
    pub async fn new(config: DiscoveryConfig) -> Result<Self> {
        Ok(Self {
            config,
            known_pairs: Arc::new(RwLock::new(HashMap::new())),
            exchange_info: Arc::new(RwLock::new(HashMap::new())),
            last_discovery_run: Arc::new(RwLock::new(chrono::Utc::now())),
        })
    }

    pub async fn discover_new_pairs(&self) -> Result<Vec<NewPairAlert>> {
        let mut all_alerts = Vec::new();
        
        // Check all major exchanges
        let exchanges = vec!["binance", "coinbase", "bybit", "okx", "kraken"];
        
        for exchange in exchanges {
            match self.check_exchange_for_new_pairs(exchange).await {
                Ok(mut alerts) => {
                    all_alerts.append(&mut alerts);
                },
                Err(e) => {
                    error!("Failed to check {} for new pairs: {}", exchange, e);
                }
            }
        }

        *self.last_discovery_run.write().await = chrono::Utc::now();
        
        info!("Discovery run completed, found {} new pairs", all_alerts.len());
        Ok(all_alerts)
    }

    async fn check_exchange_for_new_pairs(&self, exchange: &str) -> Result<Vec<NewPairAlert>> {
        info!("Checking {} for new trading pairs", exchange);
        
        // Fetch current pairs from exchange
        let current_pairs = self.fetch_exchange_pairs(exchange).await?;
        let known_pairs = self.get_known_pairs(exchange).await;
        
        // Find new pairs
        let new_pairs: Vec<_> = current_pairs
            .iter()
            .filter(|pair| !known_pairs.contains(&pair.symbol))
            .collect();

        if new_pairs.is_empty() {
            debug!("No new pairs found on {}", exchange);
            return Ok(Vec::new());
        }

        info!("Found {} new pairs on {}: {:?}", 
              new_pairs.len(), exchange, 
              new_pairs.iter().map(|p| &p.symbol).collect::<Vec<_>>());

        // Create alerts for new pairs
        let mut alerts = Vec::new();
        for pair in new_pairs {
            let priority = self.calculate_pair_priority(pair);
            
            let alert = NewPairAlert {
                exchange: exchange.to_string(),
                symbol: pair.symbol.clone(),
                base_asset: pair.base_asset.clone(),
                quote_asset: pair.quote_asset.clone(),
                detected_at: chrono::Utc::now(),
                detection_method: DetectionMethod::Manual,
                trading_start_time: pair.trading_start_time,
                priority,
            };
            
            alerts.push(alert);
        }

        // Update known pairs
        self.update_known_pairs(exchange, current_pairs.iter().map(|p| p.symbol.clone()).collect()).await;
        
        // Update exchange info
        self.update_exchange_info(exchange, &current_pairs).await;

        Ok(alerts)
    }

    async fn fetch_exchange_pairs(&self, exchange: &str) -> Result<Vec<TradingPair>> {
        // This would integrate with the actual exchange APIs from jackbot-data
        // For now, simulate with realistic data
        
        match exchange {
            "binance" => Ok(self.simulate_binance_pairs().await),
            "coinbase" => Ok(self.simulate_coinbase_pairs().await),
            "bybit" => Ok(self.simulate_bybit_pairs().await),
            "okx" => Ok(self.simulate_okx_pairs().await),
            "kraken" => Ok(self.simulate_kraken_pairs().await),
            _ => {
                warn!("Unknown exchange: {}", exchange);
                Ok(Vec::new())
            }
        }
    }

    async fn simulate_binance_pairs(&self) -> Vec<TradingPair> {
        // Simulate common pairs that would be on Binance
        let base_pairs = vec![
            ("BTC", "USDT"), ("ETH", "USDT"), ("BNB", "USDT"), ("ADA", "USDT"),
            ("SOL", "USDT"), ("DOT", "USDT"), ("AVAX", "USDT"), ("MATIC", "USDT"),
            ("LINK", "USDT"), ("UNI", "USDT"), ("AAVE", "USDT"), ("SUSHI", "USDT"),
            ("BTC", "BUSD"), ("ETH", "BUSD"), ("BNB", "BUSD"),
            ("ETH", "BTC"), ("BNB", "BTC"), ("ADA", "BTC"),
        ];

        // Occasionally add a new pair
        let mut pairs = Vec::new();
        for (base, quote) in base_pairs {
            pairs.push(TradingPair {
                symbol: format!("{}/{}", base, quote),
                base_asset: base.to_string(),
                quote_asset: quote.to_string(),
                status: "trading".to_string(),
                trading_start_time: Some(chrono::Utc::now() - chrono::Duration::days(30)),
                min_quantity: Some(0.001),
                max_quantity: Some(1000000.0),
                tick_size: Some(0.01),
                min_notional: Some(10.0),
            });
        }

        // Randomly add a new pair (10% chance)
        if rand::random::<f64>() < 0.1 {
            let new_tokens = vec!["NEWCOIN", "FRESHTOKEN", "LATEST", "TRENDING"];
            // TODO: Fix random selection - choose() method not available
            if let Some(token) = new_tokens.first() {
                pairs.push(TradingPair {
                    symbol: format!("{}/USDT", token),
                    base_asset: token.to_string(),
                    quote_asset: "USDT".to_string(),
                    status: "trading".to_string(),
                    trading_start_time: Some(chrono::Utc::now()),
                    min_quantity: Some(1.0),
                    max_quantity: Some(1000000.0),
                    tick_size: Some(0.001),
                    min_notional: Some(10.0),
                });
            }
        }

        pairs
    }

    async fn simulate_coinbase_pairs(&self) -> Vec<TradingPair> {
        let pairs = vec![
            ("BTC", "USD"), ("ETH", "USD"), ("LTC", "USD"), ("BCH", "USD"),
            ("XRP", "USD"), ("ADA", "USD"), ("DOT", "USD"), ("UNI", "USD"),
            ("LINK", "USD"), ("AAVE", "USD"), ("COMP", "USD"), ("MKR", "USD"),
            ("BTC", "EUR"), ("ETH", "EUR"), ("LTC", "EUR"),
        ];

        pairs.into_iter().map(|(base, quote)| TradingPair {
            symbol: format!("{}/{}", base, quote),
            base_asset: base.to_string(),
            quote_asset: quote.to_string(),
            status: "trading".to_string(),
            trading_start_time: Some(chrono::Utc::now() - chrono::Duration::days(60)),
            min_quantity: Some(0.001),
            max_quantity: Some(1000000.0),
            tick_size: Some(0.01),
            min_notional: Some(1.0),
        }).collect()
    }

    async fn simulate_bybit_pairs(&self) -> Vec<TradingPair> {
        let pairs = vec![
            ("BTC", "USDT"), ("ETH", "USDT"), ("SOL", "USDT"), ("AVAX", "USDT"),
            ("MATIC", "USDT"), ("DOT", "USDT"), ("ATOM", "USDT"), ("NEAR", "USDT"),
        ];

        pairs.into_iter().map(|(base, quote)| TradingPair {
            symbol: format!("{}/{}", base, quote),
            base_asset: base.to_string(),
            quote_asset: quote.to_string(),
            status: "trading".to_string(),
            trading_start_time: Some(chrono::Utc::now() - chrono::Duration::days(45)),
            min_quantity: Some(0.001),
            max_quantity: Some(1000000.0),
            tick_size: Some(0.01),
            min_notional: Some(5.0),
        }).collect()
    }

    async fn simulate_okx_pairs(&self) -> Vec<TradingPair> {
        let pairs = vec![
            ("BTC", "USDT"), ("ETH", "USDT"), ("OKB", "USDT"), ("LTC", "USDT"),
            ("XRP", "USDT"), ("ADA", "USDT"), ("DOGE", "USDT"), ("SHIB", "USDT"),
        ];

        pairs.into_iter().map(|(base, quote)| TradingPair {
            symbol: format!("{}/{}", base, quote),
            base_asset: base.to_string(),
            quote_asset: quote.to_string(),
            status: "trading".to_string(),
            trading_start_time: Some(chrono::Utc::now() - chrono::Duration::days(20)),
            min_quantity: Some(0.001),
            max_quantity: Some(1000000.0),
            tick_size: Some(0.01),
            min_notional: Some(1.0),
        }).collect()
    }

    async fn simulate_kraken_pairs(&self) -> Vec<TradingPair> {
        let pairs = vec![
            ("BTC", "USD"), ("ETH", "USD"), ("XRP", "USD"), ("LTC", "USD"),
            ("ADA", "USD"), ("DOT", "USD"), ("ATOM", "USD"), ("ALGO", "USD"),
            ("BTC", "EUR"), ("ETH", "EUR"),
        ];

        pairs.into_iter().map(|(base, quote)| TradingPair {
            symbol: format!("{}/{}", base, quote),
            base_asset: base.to_string(),
            quote_asset: quote.to_string(),
            status: "trading".to_string(),
            trading_start_time: Some(chrono::Utc::now() - chrono::Duration::days(90)),
            min_quantity: Some(0.001),
            max_quantity: Some(1000000.0),
            tick_size: Some(0.01),
            min_notional: Some(5.0),
        }).collect()
    }

    fn calculate_pair_priority(&self, pair: &TradingPair) -> AlertPriority {
        // Priority based on asset importance
        if self.config.priority_assets.contains(&pair.base_asset) {
            AlertPriority::Critical
        } else if pair.quote_asset.contains("USD") || pair.quote_asset.contains("EUR") {
            AlertPriority::High
        } else if pair.quote_asset == "USDT" || pair.quote_asset == "USDC" {
            AlertPriority::Medium
        } else {
            AlertPriority::Low
        }
    }

    async fn get_known_pairs(&self, exchange: &str) -> HashSet<String> {
        self.known_pairs.read().await
            .get(exchange)
            .cloned()
            .unwrap_or_default()
    }

    async fn update_known_pairs(&self, exchange: &str, pairs: Vec<String>) {
        let pair_set: HashSet<String> = pairs.into_iter().collect();
        self.known_pairs.write().await.insert(exchange.to_string(), pair_set);
    }

    async fn update_exchange_info(&self, exchange: &str, current_pairs: &[TradingPair]) {
        let known_pairs = self.get_known_pairs(exchange).await;
        let total_pairs = current_pairs.len();
        let monitored_pairs = known_pairs.len();
        let new_pairs_detected = total_pairs.saturating_sub(monitored_pairs);
        let coverage_percentage = if total_pairs > 0 {
            (monitored_pairs as f64 / total_pairs as f64) * 100.0
        } else {
            100.0
        };

        let info = ExchangeInfo {
            exchange: exchange.to_string(),
            total_pairs,
            monitored_pairs,
            new_pairs_detected,
            last_check: chrono::Utc::now(),
            coverage_percentage,
        };

        self.exchange_info.write().await.insert(exchange.to_string(), info);
    }

    pub async fn start_discovery(&self, exchange: &str, interval_secs: u64) -> Result<()> {
        info!("Starting discovery for {} with {}s interval", exchange, interval_secs);
        
        let discovery = self.clone();
        let exchange = exchange.to_string();
        
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(interval_secs));
            
            loop {
                interval.tick().await;
                
                if let Err(e) = discovery.check_exchange_for_new_pairs(&exchange).await {
                    error!("Discovery failed for {}: {}", exchange, e);
                }
            }
        });
        
        Ok(())
    }

    pub async fn update_exchange_symbols(&self, exchange: &str) -> Result<()> {
        info!("Updating symbols for {}", exchange);
        
        // Force refresh of pairs for this exchange
        let pairs = self.fetch_exchange_pairs(exchange).await?;
        self.update_known_pairs(exchange, pairs.iter().map(|p| p.symbol.clone()).collect()).await;
        self.update_exchange_info(exchange, &pairs).await;
        
        info!("Updated {} symbols for {}", pairs.len(), exchange);
        Ok(())
    }

    pub async fn get_exchange_info(&self, exchange: &str) -> Option<ExchangeInfo> {
        self.exchange_info.read().await.get(exchange).cloned()
    }

    pub async fn get_coverage_metrics(&self) -> HashMap<String, ExchangeInfo> {
        self.exchange_info.read().await.clone()
    }

    pub async fn get_total_pairs_count(&self) -> usize {
        self.known_pairs.read().await
            .values()
            .map(|pairs| pairs.len())
            .sum()
    }

    pub async fn check_coverage_threshold(&self, exchange: &str) -> bool {
        if let Some(info) = self.get_exchange_info(exchange).await {
            info.coverage_percentage >= self.config.coverage_threshold
        } else {
            false
        }
    }

    pub async fn get_missing_pairs(&self, exchange: &str) -> Result<Vec<String>> {
        let current_pairs = self.fetch_exchange_pairs(exchange).await?;
        let known_pairs = self.get_known_pairs(exchange).await;
        
        let missing: Vec<String> = current_pairs
            .iter()
            .filter_map(|pair| {
                if !known_pairs.contains(&pair.symbol) {
                    Some(pair.symbol.clone())
                } else {
                    None
                }
            })
            .collect();
        
        Ok(missing)
    }
}

// Import rand for simulation
use rand::seq::SliceRandom;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_pair_discovery_creation() {
        let config = DiscoveryConfig {
            check_interval: 300,
            new_pair_alert_channels: vec!["test".to_string()],
            coverage_threshold: 99.0,
            priority_assets: vec!["BTC".to_string(), "ETH".to_string()],
        };

        let discovery = PairDiscovery::new(config).await.unwrap();
        let total_pairs = discovery.get_total_pairs_count().await;
        assert_eq!(total_pairs, 0); // Should start empty
    }

    #[tokio::test]
    async fn test_new_pair_detection() {
        let config = DiscoveryConfig {
            check_interval: 300,
            new_pair_alert_channels: vec!["test".to_string()],
            coverage_threshold: 99.0,
            priority_assets: vec!["BTC".to_string(), "ETH".to_string()],
        };

        let discovery = PairDiscovery::new(config).await.unwrap();
        
        // First run should detect all pairs as new
        let alerts = discovery.discover_new_pairs().await.unwrap();
        assert!(!alerts.is_empty());

        // Second run should detect fewer new pairs
        let alerts2 = discovery.discover_new_pairs().await.unwrap();
        assert!(alerts2.len() <= alerts.len());
    }

    #[tokio::test]
    async fn test_priority_calculation() {
        let config = DiscoveryConfig {
            check_interval: 300,
            new_pair_alert_channels: vec!["test".to_string()],
            coverage_threshold: 99.0,
            priority_assets: vec!["BTC".to_string(), "ETH".to_string()],
        };

        let discovery = PairDiscovery::new(config).await.unwrap();

        let btc_pair = TradingPair {
            symbol: "BTC/USDT".to_string(),
            base_asset: "BTC".to_string(),
            quote_asset: "USDT".to_string(),
            status: "trading".to_string(),
            trading_start_time: None,
            min_quantity: None,
            max_quantity: None,
            tick_size: None,
            min_notional: None,
        };

        let priority = discovery.calculate_pair_priority(&btc_pair);
        assert!(matches!(priority, AlertPriority::Critical));

        let random_pair = TradingPair {
            symbol: "RANDOM/BTC".to_string(),
            base_asset: "RANDOM".to_string(),
            quote_asset: "BTC".to_string(),
            status: "trading".to_string(),
            trading_start_time: None,
            min_quantity: None,
            max_quantity: None,
            tick_size: None,
            min_notional: None,
        };

        let priority2 = discovery.calculate_pair_priority(&random_pair);
        assert!(matches!(priority2, AlertPriority::Low));
    }
}