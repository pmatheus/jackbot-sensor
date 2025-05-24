//! Market type and normalization for Hyperliquid.

use serde::{Deserialize, Serialize};

/// Market identifier for Hyperliquid WebSocket subscriptions (e.g., "BTC").
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct HyperliquidMarket(pub String);

impl AsRef<str> for HyperliquidMarket {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<&str> for HyperliquidMarket {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

impl From<String> for HyperliquidMarket {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl std::fmt::Display for HyperliquidMarket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl crate::Identifier<HyperliquidMarket> for HyperliquidMarket {
    fn id(&self) -> HyperliquidMarket {
        self.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_market_from_str() {
        let m = HyperliquidMarket::from("BTC");
        assert_eq!(m.as_ref(), "BTC");
        assert_eq!(m.to_string(), "BTC");
    }
}
