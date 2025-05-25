use jackbot_integration::{Validator, error::SocketError};
use serde::{Deserialize, Serialize};

/// Subscription response type for MEXC WebSocket API.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MexcSubResponse {
    #[serde(default)]
    pub success: bool,
}

impl Validator for MexcSubResponse {
    fn validate(self) -> Result<Self, SocketError> {
        if self.success {
            Ok(self)
        } else {
            Err(SocketError::Subscribe("received failure subscription response".to_owned()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mexc_sub_response_validate() {
        let ok = MexcSubResponse { success: true };
        assert!(ok.validate().is_ok());

        let err = MexcSubResponse { success: false };
        assert!(err.validate().is_err());
    }
}
