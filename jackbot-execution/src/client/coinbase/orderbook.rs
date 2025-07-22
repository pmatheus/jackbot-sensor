use crate::error::UnindexedClientError;
use jackbot_data::books::{Level, OrderBook as DataOrderBook};
use rust_decimal::Decimal;
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, warn};

/// Represents a snapshot of the order book at a specific sequence
#[derive(Debug, Clone)]
pub struct OrderBookSnapshot {
    pub product_id: String,
    pub sequence: u64,
    pub bids: Vec<(Decimal, Decimal)>, // (price, size)
    pub asks: Vec<(Decimal, Decimal)>, // (price, size)
}

/// Represents an incremental update to the order book
#[derive(Debug, Clone)]
pub struct OrderBookUpdate {
    pub sequence: u64,
    pub side: String, // "buy" or "sell"
    pub price: Decimal,
    pub size: Decimal, // 0 means remove the level
}

/// Thread-safe Coinbase order book with atomic updates
#[derive(Clone)]
pub struct CoinbaseOrderBook {
    product_id: String,
    inner: Arc<RwLock<OrderBookInner>>,
}

#[derive(Debug)]
struct OrderBookInner {
    sequence: u64,
    bids: BTreeMap<Decimal, Decimal>, // Price -> Size, automatically sorted
    asks: BTreeMap<Decimal, Decimal>, // Price -> Size, automatically sorted
}

impl CoinbaseOrderBook {
    /// Create a new order book for a specific product
    pub fn new(product_id: &str) -> Self {
        Self {
            product_id: product_id.to_string(),
            inner: Arc::new(RwLock::new(OrderBookInner {
                sequence: 0,
                bids: BTreeMap::new(),
                asks: BTreeMap::new(),
            })),
        }
    }

    /// Apply a snapshot to the order book (replaces all data)
    pub async fn apply_snapshot(&self, snapshot: OrderBookSnapshot) {
        let mut inner = self.inner.write().await;
        
        // Clear existing data
        inner.bids.clear();
        inner.asks.clear();
        
        // Update sequence
        inner.sequence = snapshot.sequence;
        
        // Insert bids
        for (price, size) in snapshot.bids {
            if size > Decimal::ZERO {
                inner.bids.insert(price, size);
            }
        }
        
        // Insert asks
        for (price, size) in snapshot.asks {
            if size > Decimal::ZERO {
                inner.asks.insert(price, size);
            }
        }
        
        debug!(
            "Applied snapshot for {} at sequence {}: {} bids, {} asks",
            self.product_id,
            inner.sequence,
            inner.bids.len(),
            inner.asks.len()
        );
    }

    /// Apply an incremental update to the order book
    pub async fn apply_update(&self, update: OrderBookUpdate) -> Result<(), UnindexedClientError> {
        let mut inner = self.inner.write().await;
        
        // Validate sequence
        if update.sequence <= inner.sequence {
            return Err(UnindexedClientError::AccountStream(
                format!("Out of order update: current={}, received={}", inner.sequence, update.sequence)
            ));
        }
        
        // Check for sequence gap
        if update.sequence > inner.sequence + 1 {
            return Err(UnindexedClientError::AccountStream(
                format!("Sequence gap detected: current={}, received={}", inner.sequence, update.sequence)
            ));
        }
        
        // Update sequence
        inner.sequence = update.sequence;
        
        // Apply the update
        match update.side.as_str() {
            "buy" => {
                if update.size > Decimal::ZERO {
                    inner.bids.insert(update.price, update.size);
                } else {
                    inner.bids.remove(&update.price);
                }
            }
            "sell" => {
                if update.size > Decimal::ZERO {
                    inner.asks.insert(update.price, update.size);
                } else {
                    inner.asks.remove(&update.price);
                }
            }
            _ => {
                warn!("Unknown order side: {}", update.side);
            }
        }
        
        Ok(())
    }

    /// Get a snapshot of the current order book state
    pub async fn get_snapshot(&self) -> OrderBookSnapshot {
        let inner = self.inner.read().await;
        
        OrderBookSnapshot {
            product_id: self.product_id.clone(),
            sequence: inner.sequence,
            bids: inner.bids.iter().rev().map(|(&p, &s)| (p, s)).collect(),
            asks: inner.asks.iter().map(|(&p, &s)| (p, s)).collect(),
        }
    }

    /// Convert to the data layer OrderBook format
    pub async fn to_data_orderbook(&self) -> DataOrderBook {
        let inner = self.inner.read().await;
        
        // Convert bids (sorted descending)
        let bid_levels: Vec<Level> = inner.bids
            .iter()
            .rev()
            .map(|(&price, &amount)| Level::new(price, amount))
            .collect();
        
        // Convert asks (sorted ascending)
        let ask_levels: Vec<Level> = inner.asks
            .iter()
            .map(|(&price, &amount)| Level::new(price, amount))
            .collect();
        
        DataOrderBook::new(
            inner.sequence,
            Some(chrono::Utc::now()),
            bid_levels,
            ask_levels,
        )
    }

    /// Get the best bid and ask prices and sizes
    pub async fn get_best_bid_ask(&self) -> (Option<(Decimal, Decimal)>, Option<(Decimal, Decimal)>) {
        let inner = self.inner.read().await;
        
        let best_bid = inner.bids.iter().rev().next().map(|(&p, &s)| (p, s));
        let best_ask = inner.asks.iter().next().map(|(&p, &s)| (p, s));
        
        (best_bid, best_ask)
    }

    /// Calculate the spread
    pub async fn get_spread(&self) -> Option<Decimal> {
        let (best_bid, best_ask) = self.get_best_bid_ask().await;
        
        match (best_bid, best_ask) {
            (Some((bid_price, _)), Some((ask_price, _))) => Some(ask_price - bid_price),
            _ => None,
        }
    }

    /// Calculate the spread in basis points
    pub async fn get_spread_bps(&self) -> Option<Decimal> {
        let (best_bid, best_ask) = self.get_best_bid_ask().await;
        
        match (best_bid, best_ask) {
            (Some((bid_price, _)), Some((ask_price, _))) => {
                let spread = ask_price - bid_price;
                let mid_price = (bid_price + ask_price) / Decimal::from(2);
                Some(spread / mid_price * Decimal::from(10000))
            }
            _ => None,
        }
    }

    /// Calculate a simple checksum for order book integrity
    pub async fn calculate_checksum(&self) -> u64 {
        let inner = self.inner.read().await;
        
        let mut checksum = 0u64;
        
        // Include top 10 bids
        for (i, (&price, &size)) in inner.bids.iter().rev().take(10).enumerate() {
            checksum = checksum.wrapping_add(
                ((price.mantissa() as u64) << 32) | (size.mantissa() as u64)
            ).wrapping_add(i as u64);
        }
        
        // Include top 10 asks
        for (i, (&price, &size)) in inner.asks.iter().take(10).enumerate() {
            checksum = checksum.wrapping_add(
                ((price.mantissa() as u64) << 32) | (size.mantissa() as u64)
            ).wrapping_add((i + 100) as u64);
        }
        
        checksum.wrapping_add(inner.sequence)
    }

    /// Get market depth statistics
    pub async fn get_depth_stats(&self, levels: usize) -> (Decimal, Decimal) {
        let inner = self.inner.read().await;
        
        let bid_depth: Decimal = inner.bids
            .iter()
            .rev()
            .take(levels)
            .map(|(_, &size)| size)
            .sum();
        
        let ask_depth: Decimal = inner.asks
            .iter()
            .take(levels)
            .map(|(_, &size)| size)
            .sum();
        
        (bid_depth, ask_depth)
    }

    /// Check if the order book is healthy (has both bids and asks)
    pub async fn is_healthy(&self) -> bool {
        let inner = self.inner.read().await;
        !inner.bids.is_empty() && !inner.asks.is_empty()
    }

    /// Get the current sequence number
    pub async fn get_sequence(&self) -> u64 {
        self.inner.read().await.sequence
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[tokio::test]
    async fn test_orderbook_operations() {
        let orderbook = CoinbaseOrderBook::new("BTC-USD");
        
        // Apply snapshot
        let snapshot = OrderBookSnapshot {
            product_id: "BTC-USD".to_string(),
            sequence: 1000,
            bids: vec![
                (Decimal::from_str("50000.00").unwrap(), Decimal::from_str("1.0").unwrap()),
                (Decimal::from_str("49999.00").unwrap(), Decimal::from_str("2.0").unwrap()),
            ],
            asks: vec![
                (Decimal::from_str("50001.00").unwrap(), Decimal::from_str("1.0").unwrap()),
                (Decimal::from_str("50002.00").unwrap(), Decimal::from_str("2.0").unwrap()),
            ],
        };
        
        orderbook.apply_snapshot(snapshot).await;
        
        // Verify snapshot was applied
        let (best_bid, best_ask) = orderbook.get_best_bid_ask().await;
        assert_eq!(best_bid.unwrap().0, Decimal::from_str("50000.00").unwrap());
        assert_eq!(best_ask.unwrap().0, Decimal::from_str("50001.00").unwrap());
        
        // Apply update
        let update = OrderBookUpdate {
            sequence: 1001,
            side: "buy".to_string(),
            price: Decimal::from_str("50000.50").unwrap(),
            size: Decimal::from_str("3.0").unwrap(),
        };
        
        orderbook.apply_update(update).await.unwrap();
        
        // Verify update was applied
        let (best_bid, _) = orderbook.get_best_bid_ask().await;
        assert_eq!(best_bid.unwrap().0, Decimal::from_str("50000.50").unwrap());
    }
}