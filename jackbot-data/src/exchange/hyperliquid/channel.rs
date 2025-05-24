//! Channel type and normalization for Hyperliquid.

use serde::{Deserialize, Serialize};

/// Channel identifier for Hyperliquid WebSocket subscriptions.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct HyperliquidChannel(pub &'static str);

impl AsRef<str> for HyperliquidChannel {
    fn as_ref(&self) -> &str {
        self.0
    }
}

impl HyperliquidChannel {
    /// Top of book updates.
    pub const ORDER_BOOK_L1: Self = Self("l1");
    /// Full order book updates.
    pub const ORDER_BOOK_L2: Self = Self("l2");
    /// Public trade events.
    pub const TRADES: Self = Self("trades");
    /// Liquidation events.
    pub const LIQUIDATIONS: Self = Self("liquidations");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channel_consts() {
        assert_eq!(HyperliquidChannel::ORDER_BOOK_L1.as_ref(), "l1");
        assert_eq!(HyperliquidChannel::ORDER_BOOK_L2.as_ref(), "l2");
        assert_eq!(HyperliquidChannel::TRADES.as_ref(), "trades");
        assert_eq!(HyperliquidChannel::LIQUIDATIONS.as_ref(), "liquidations");
    }
}
