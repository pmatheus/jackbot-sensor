use crate::subscription::liquidation::Liquidation;
use jackbot_integration::subscription::SubscriptionId;

/// OKX does not support a public liquidations channel as of 2024-06.
/// This module is a stub for feature parity and will not emit any events.
pub struct OkxLiquidation;

impl OkxLiquidation {
    /// OKX does not expose liquidation events publicly.
    pub fn from_message(_msg: &str) -> Option<()> {
        // No public liquidations channel on OKX
        None
    }

    /// Normalization helper that always returns `None`.
    pub fn normalize(&self) -> Option<Liquidation> {
        None
    }
}

impl crate::Identifier<Option<SubscriptionId>> for OkxLiquidation {
    fn id(&self) -> Option<SubscriptionId> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_okx_liquidation_stub() {
        let liq = OkxLiquidation;
        assert!(liq.normalize().is_none());
        assert!(OkxLiquidation::from_message("{}").is_none());
        assert!(liq.id().is_none());
    }
}

