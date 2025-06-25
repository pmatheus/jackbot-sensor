//! Subscription logic for Hyperliquid.

use jackbot_integration::{Validator, error::SocketError};
use serde::{Deserialize, Serialize};

/// Subscription response type for Hyperliquid WebSocket API.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HyperliquidSubResponse {
    pub success: bool,
}

impl Validator for HyperliquidSubResponse {
    fn validate(self) -> Result<Self, SocketError> {
        if self.success {
            Ok(self)
        } else {
            Err(SocketError::Subscribe(
                "received failure subscription response".to_owned(),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hyperliquid_sub_response_validate() {
        let ok = HyperliquidSubResponse { success: true };
        assert!(ok.validate().is_ok());

        let err = HyperliquidSubResponse { success: false };
        assert!(err.validate().is_err());
    }
}

// TODO: Implement subscription logic.
