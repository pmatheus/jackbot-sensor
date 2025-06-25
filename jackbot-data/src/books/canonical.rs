//! Canonical representation of orderbook data.
//!
//! This module provides a standardized representation for orderbook data from various exchanges.
//! Rather than "normalization" (which has specific meaning in machine learning contexts),
//! we use the term "canonicalization" to refer to the process of converting exchange-specific
//! orderbook formats into this standard representation.

use super::OrderBook;
use chrono::{DateTime, Utc};
use rust_decimal::prelude::ToPrimitive;

/// Trait for converting exchange-specific orderbook data into a canonical (standardized) form.
///
/// By implementing this trait, exchange-specific orderbook formats can be converted
/// into a single, consistent representation that can be used throughout the system.
pub trait Canonicalizer {
    /// Convert exchange-specific data into the canonical `OrderBook` form.
    fn canonicalize(&self, timestamp: DateTime<Utc>) -> OrderBook;
}

/// Wraps a canonicalized orderbook to provide additional functionality.
#[derive(Debug, Clone)]
pub struct CanonicalOrderBook {
    inner: OrderBook,
}

impl CanonicalOrderBook {
    /// Create a new canonical orderbook from an `OrderBook`.
    pub fn new(orderbook: OrderBook) -> Self {
        Self { inner: orderbook }
    }

    /// Get the underlying OrderBook.
    pub fn into_inner(self) -> OrderBook {
        self.inner
    }

    /// Get a reference to the underlying OrderBook.
    pub fn inner(&self) -> &OrderBook {
        &self.inner
    }

    /// Calculate the mid price of the orderbook.
    pub fn mid_price(&self) -> Option<f64> {
        let bids = self.inner.bids().levels();
        let asks = self.inner.asks().levels();

        if bids.is_empty() || asks.is_empty() {
            return None;
        }

        let best_bid = bids[0].price.to_f64().unwrap_or_default();
        let best_ask = asks[0].price.to_f64().unwrap_or_default();

        Some((best_bid + best_ask) / 2.0)
    }

    /// Calculate the spread of the orderbook.
    pub fn spread(&self) -> Option<f64> {
        let bids = self.inner.bids().levels();
        let asks = self.inner.asks().levels();

        if bids.is_empty() || asks.is_empty() {
            return None;
        }

        let best_bid = bids[0].price.to_f64().unwrap_or_default();
        let best_ask = asks[0].price.to_f64().unwrap_or_default();

        Some(best_ask - best_bid)
    }

    /// Calculate the relative spread as a percentage.
    pub fn relative_spread(&self) -> Option<f64> {
        let spread = self.spread()?;
        let mid_price = self.mid_price()?;

        Some((spread / mid_price) * 100.0)
    }

    /// Get the total volume at the specified depth.
    pub fn volume_at_depth(&self, depth: usize) -> (f64, f64) {
        let bids = self.inner.bids().levels();
        let asks = self.inner.asks().levels();

        let bid_volume = bids
            .iter()
            .take(depth)
            .map(|level| level.amount.to_f64().unwrap_or_default())
            .sum();

        let ask_volume = asks
            .iter()
            .take(depth)
            .map(|level| level.amount.to_f64().unwrap_or_default())
            .sum();

        (bid_volume, ask_volume)
    }
}

impl From<OrderBook> for CanonicalOrderBook {
    fn from(orderbook: OrderBook) -> Self {
        Self::new(orderbook)
    }
}

impl From<CanonicalOrderBook> for OrderBook {
    fn from(canonical: CanonicalOrderBook) -> Self {
        canonical.into_inner()
    }
}
