use serde::{Deserialize, Serialize};

/// Channel identifier for MEXC WebSocket subscriptions.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Serialize, Deserialize)]
pub struct MexcChannel(pub &'static str);

impl MexcChannel {
    /// Public trade events channel.
    pub const TRADES: Self = Self("spot@public.deals.v3.api");
}

impl AsRef<str> for MexcChannel {
    fn as_ref(&self) -> &str {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channel_const() {
        assert_eq!(MexcChannel::TRADES.as_ref(), "spot@public.deals.v3.api");
    }
}
