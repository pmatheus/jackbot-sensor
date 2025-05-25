use serde::{Deserialize, Serialize};

/// Market identifier for MEXC WebSocket subscriptions (eg. "BTC_USDT").
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MexcMarket(pub String);

impl AsRef<str> for MexcMarket {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<&str> for MexcMarket {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

impl From<String> for MexcMarket {
    fn from(s: String) -> Self {
        Self(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_market_from_str() {
        let m = MexcMarket::from("BTC_USDT");
        assert_eq!(m.as_ref(), "BTC_USDT");
    }
}
