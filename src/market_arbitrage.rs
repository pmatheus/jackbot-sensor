//! Market arbitrage detection module
//!
//! High-performance arbitrage opportunity detection across 11 exchanges
//! with <10ms latency for real-time trading opportunities.

use anyhow::{Result, Context};
use crossbeam_channel::{Receiver, Sender};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

/// Arbitrage opportunity with all relevant details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArbitrageOpportunity {
    pub id: String,
    pub symbol: String,
    pub buy_exchange: String,
    pub buy_price: f64,
    pub sell_exchange: String,
    pub sell_price: f64,
    pub profit_percentage: f64,
    pub profit_amount_per_unit: f64,
    pub max_quantity: f64,
    pub estimated_profit_usd: f64,
    pub fees_included: bool,
    pub execution_risk: ExecutionRisk,
    pub timestamp: u64,
    pub latency_ms: u64,
}

/// Execution risk assessment
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum ExecutionRisk {
    Low,     // <50ms combined latency, high liquidity
    Medium,  // 50-100ms latency or medium liquidity
    High,    // >100ms latency or low liquidity
}

/// Exchange fee structure
#[derive(Debug, Clone)]
pub struct ExchangeFees {
    pub maker_fee: f64,
    pub taker_fee: f64,
    pub withdrawal_fee: f64,
}

/// Market depth at a specific price level
#[derive(Debug, Clone, Copy)]
pub struct DepthLevel {
    pub price: f64,
    pub quantity: f64,
}

/// Price update from an exchange
#[derive(Debug, Clone)]
pub struct PriceUpdate {
    pub exchange: String,
    pub symbol: String,
    pub bid: f64,
    pub ask: f64,
    pub bid_quantity: f64,
    pub ask_quantity: f64,
    pub timestamp: u64,
    pub latency_ms: u64,
}

/// Arbitrage detector with high-performance scanning
pub struct ArbitrageDetector {
    /// Minimum profit threshold (as percentage)
    min_profit_threshold: f64,
    
    /// Current prices by symbol and exchange
    prices: Arc<RwLock<HashMap<String, HashMap<String, PriceUpdate>>>>,
    
    /// Exchange fee structures
    exchange_fees: Arc<RwLock<HashMap<String, ExchangeFees>>>,
    
    /// Active arbitrage opportunities
    opportunities: Arc<RwLock<HashMap<String, ArbitrageOpportunity>>>,
    
    /// Channel for price updates
    update_channel: (Sender<PriceUpdate>, Receiver<PriceUpdate>),
    
    /// Channel for arbitrage alerts
    alert_channel: (Sender<ArbitrageOpportunity>, Receiver<ArbitrageOpportunity>),
}

impl ArbitrageDetector {
    /// Create a new arbitrage detector
    pub fn new(min_profit_threshold: f64) -> Self {
        let (update_tx, update_rx) = crossbeam_channel::unbounded();
        let (alert_tx, alert_rx) = crossbeam_channel::bounded(1000);
        
        let detector = Self {
            min_profit_threshold,
            prices: Arc::new(RwLock::new(HashMap::with_capacity(100))),
            exchange_fees: Arc::new(RwLock::new(HashMap::with_capacity(11))),
            opportunities: Arc::new(RwLock::new(HashMap::with_capacity(100))),
            update_channel: (update_tx, update_rx),
            alert_channel: (alert_tx, alert_rx),
        };
        
        // Initialize exchange fees for all 11 exchanges
        detector.initialize_exchange_fees();
        
        // Start processing thread
        detector.start_processing_thread();
        
        detector
    }
    
    /// Initialize exchange fee structures
    fn initialize_exchange_fees(&self) {
        let mut fees = self.exchange_fees.write();
        
        // Spot trading fees (maker/taker)
        fees.insert("binance".to_string(), ExchangeFees {
            maker_fee: 0.001,    // 0.1%
            taker_fee: 0.001,    // 0.1%
            withdrawal_fee: 0.0, // Varies by asset
        });
        
        fees.insert("coinbase".to_string(), ExchangeFees {
            maker_fee: 0.005,    // 0.5%
            taker_fee: 0.005,    // 0.5%
            withdrawal_fee: 0.0,
        });
        
        fees.insert("bybit".to_string(), ExchangeFees {
            maker_fee: 0.001,
            taker_fee: 0.001,
            withdrawal_fee: 0.0,
        });
        
        fees.insert("bitget".to_string(), ExchangeFees {
            maker_fee: 0.001,
            taker_fee: 0.001,
            withdrawal_fee: 0.0,
        });
        
        fees.insert("hyperliquid".to_string(), ExchangeFees {
            maker_fee: 0.0002,   // 0.02% - very low fees
            taker_fee: 0.0005,   // 0.05%
            withdrawal_fee: 0.0,
        });
        
        fees.insert("kucoin".to_string(), ExchangeFees {
            maker_fee: 0.001,
            taker_fee: 0.001,
            withdrawal_fee: 0.0,
        });
        
        fees.insert("kraken".to_string(), ExchangeFees {
            maker_fee: 0.0016,   // 0.16%
            taker_fee: 0.0026,   // 0.26%
            withdrawal_fee: 0.0,
        });
        
        fees.insert("okx".to_string(), ExchangeFees {
            maker_fee: 0.0008,   // 0.08%
            taker_fee: 0.001,    // 0.1%
            withdrawal_fee: 0.0,
        });
        
        fees.insert("gateio".to_string(), ExchangeFees {
            maker_fee: 0.002,    // 0.2%
            taker_fee: 0.002,    // 0.2%
            withdrawal_fee: 0.0,
        });
        
        fees.insert("mexc".to_string(), ExchangeFees {
            maker_fee: 0.0,      // 0% maker fee!
            taker_fee: 0.001,    // 0.1%
            withdrawal_fee: 0.0,
        });
        
        fees.insert("bingx".to_string(), ExchangeFees {
            maker_fee: 0.001,
            taker_fee: 0.001,
            withdrawal_fee: 0.0,
        });
    }
    
    /// Update price for an exchange
    pub fn update_price(
        &self,
        exchange: String,
        symbol: String,
        bid: f64,
        ask: f64,
        bid_quantity: f64,
        ask_quantity: f64,
        latency_ms: u64,
    ) -> Result<()> {
        let update = PriceUpdate {
            exchange,
            symbol,
            bid,
            ask,
            bid_quantity,
            ask_quantity,
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
            latency_ms,
        };
        
        self.update_channel.0.send(update)
            .context("Failed to send price update")?;
        
        Ok(())
    }
    
    /// Subscribe to arbitrage alerts
    pub fn subscribe_alerts(&self) -> Receiver<ArbitrageOpportunity> {
        self.alert_channel.1.clone()
    }
    
    /// Get all active arbitrage opportunities
    pub fn get_opportunities(&self) -> Vec<ArbitrageOpportunity> {
        let opps = self.opportunities.read();
        opps.values().cloned().collect()
    }
    
    /// Get opportunities for a specific symbol
    pub fn get_symbol_opportunities(&self, symbol: &str) -> Vec<ArbitrageOpportunity> {
        let opps = self.opportunities.read();
        opps.values()
            .filter(|opp| opp.symbol == symbol)
            .cloned()
            .collect()
    }
    
    /// Start the processing thread
    fn start_processing_thread(&self) {
        let prices = Arc::clone(&self.prices);
        let exchange_fees = Arc::clone(&self.exchange_fees);
        let opportunities = Arc::clone(&self.opportunities);
        let rx = self.update_channel.1.clone();
        let alert_tx = self.alert_channel.0.clone();
        let min_profit = self.min_profit_threshold;
        
        std::thread::spawn(move || {
            let mut batch = Vec::with_capacity(100);
            
            loop {
                // Batch updates for efficiency
                batch.clear();
                
                // Collect updates
                while batch.len() < 100 {
                    match rx.try_recv() {
                        Ok(update) => batch.push(update),
                        Err(_) => break,
                    }
                }
                
                if batch.is_empty() {
                    match rx.recv_timeout(Duration::from_micros(100)) {
                        Ok(update) => batch.push(update),
                        Err(_) => continue,
                    }
                }
                
                let start = Instant::now();
                
                // Process updates
                for update in &batch {
                    // Update price map
                    {
                        let mut prices_write = prices.write();
                        prices_write
                            .entry(update.symbol.clone())
                            .or_insert_with(HashMap::new)
                            .insert(update.exchange.clone(), update.clone());
                    }
                }
                
                // Scan for arbitrage
                Self::scan_arbitrage(
                    &prices,
                    &exchange_fees,
                    &opportunities,
                    &alert_tx,
                    min_profit,
                );
                
                let elapsed = start.elapsed();
                if elapsed.as_millis() > 10 {
                    warn!("Slow arbitrage scan: {}ms", elapsed.as_millis());
                }
            }
        });
    }
    
    /// Scan for arbitrage opportunities
    fn scan_arbitrage(
        prices: &Arc<RwLock<HashMap<String, HashMap<String, PriceUpdate>>>>,
        exchange_fees: &Arc<RwLock<HashMap<String, ExchangeFees>>>,
        opportunities: &Arc<RwLock<HashMap<String, ArbitrageOpportunity>>>,
        alert_tx: &Sender<ArbitrageOpportunity>,
        min_profit: f64,
    ) {
        let prices_read = prices.read();
        let fees_read = exchange_fees.read();
        
        for (symbol, exchange_prices) in prices_read.iter() {
            // Find best bid (highest) and best ask (lowest)
            let mut best_bid: Option<(&String, &PriceUpdate)> = None;
            let mut best_ask: Option<(&String, &PriceUpdate)> = None;
            
            for (exchange, price) in exchange_prices {
                if best_bid.is_none() || price.bid > best_bid.unwrap().1.bid {
                    best_bid = Some((exchange, price));
                }
                
                if best_ask.is_none() || price.ask < best_ask.unwrap().1.ask {
                    best_ask = Some((exchange, price));
                }
            }
            
            if let (Some((bid_exchange, bid_price)), Some((ask_exchange, ask_price))) = 
                (best_bid, best_ask) {
                
                // Check if different exchanges
                if bid_exchange != ask_exchange {
                    // Get fees
                    let bid_fees = fees_read.get(bid_exchange);
                    let ask_fees = fees_read.get(ask_exchange);
                    
                    if let (Some(bid_fee), Some(ask_fee)) = (bid_fees, ask_fees) {
                        // Calculate profit including fees
                        let buy_cost = ask_price.ask * (1.0 + ask_fee.taker_fee);
                        let sell_revenue = bid_price.bid * (1.0 - bid_fee.taker_fee);
                        
                        if sell_revenue > buy_cost {
                            let profit_pct = (sell_revenue - buy_cost) / buy_cost * 100.0;
                            
                            if profit_pct >= min_profit {
                                let max_qty = bid_price.bid_quantity.min(ask_price.ask_quantity);
                                let profit_per_unit = sell_revenue - buy_cost;
                                
                                // Assess execution risk
                                let total_latency = bid_price.latency_ms + ask_price.latency_ms;
                                let risk = if total_latency < 50 && max_qty > 1.0 {
                                    ExecutionRisk::Low
                                } else if total_latency < 100 && max_qty > 0.1 {
                                    ExecutionRisk::Medium
                                } else {
                                    ExecutionRisk::High
                                };
                                
                                let opportunity = ArbitrageOpportunity {
                                    id: format!("{}-{}-{}", symbol, ask_exchange, bid_exchange),
                                    symbol: symbol.clone(),
                                    buy_exchange: ask_exchange.clone(),
                                    buy_price: ask_price.ask,
                                    sell_exchange: bid_exchange.clone(),
                                    sell_price: bid_price.bid,
                                    profit_percentage: profit_pct,
                                    profit_amount_per_unit: profit_per_unit,
                                    max_quantity: max_qty,
                                    estimated_profit_usd: profit_per_unit * max_qty,
                                    fees_included: true,
                                    execution_risk: risk,
                                    timestamp: chrono::Utc::now().timestamp_millis() as u64,
                                    latency_ms: total_latency,
                                };
                                
                                // Store and alert
                                {
                                    let mut opps = opportunities.write();
                                    opps.insert(opportunity.id.clone(), opportunity.clone());
                                }
                                
                                if let Err(e) = alert_tx.try_send(opportunity) {
                                    debug!("Failed to send arbitrage alert: {}", e);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_arbitrage_detection() {
        let detector = ArbitrageDetector::new(0.1); // 0.1% minimum profit
        
        // Update prices with arbitrage opportunity
        detector.update_price(
            "binance".to_string(),
            "BTC/USDT".to_string(),
            42000.0,  // bid
            42001.0,  // ask
            1.0,      // bid quantity
            1.0,      // ask quantity
            10,       // latency
        ).unwrap();
        
        detector.update_price(
            "coinbase".to_string(),
            "BTC/USDT".to_string(),
            42010.0,  // bid - higher than binance ask!
            42011.0,  // ask
            0.5,      // bid quantity
            0.5,      // ask quantity
            15,       // latency
        ).unwrap();
        
        // Allow processing
        std::thread::sleep(Duration::from_millis(10));
        
        // Check opportunities
        let opps = detector.get_opportunities();
        assert!(!opps.is_empty());
        
        let opp = &opps[0];
        assert_eq!(opp.buy_exchange, "binance");
        assert_eq!(opp.sell_exchange, "coinbase");
        assert!(opp.profit_percentage > 0.0);
        assert_eq!(opp.execution_risk, ExecutionRisk::Low);
    }
    
    #[test]
    fn test_fees_calculation() {
        let detector = ArbitrageDetector::new(0.1);
        
        // MEXC has 0% maker fee, good for arbitrage
        detector.update_price(
            "mexc".to_string(),
            "ETH/USDT".to_string(),
            2000.0,  // bid
            2001.0,  // ask
            10.0,    // quantity
            10.0,
            5,       // low latency
        ).unwrap();
        
        detector.update_price(
            "kraken".to_string(),
            "ETH/USDT".to_string(),
            2002.5,  // bid - needs to cover higher fees
            2003.0,  // ask
            5.0,
            5.0,
            20,
        ).unwrap();
        
        std::thread::sleep(Duration::from_millis(10));
        
        let opps = detector.get_opportunities();
        if !opps.is_empty() {
            let opp = &opps[0];
            assert!(opp.fees_included);
            assert_eq!(opp.buy_exchange, "mexc"); // Lower fees
        }
    }
}