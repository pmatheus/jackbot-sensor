//! Subscription logic and types for Kucoin exchange.

use jackbot_integration::{Validator, error::SocketError};
use serde::{Deserialize, Serialize};

/// [`Kucoin`](super::Kucoin) WebSocket subscription response.
///
/// ### Raw Payload Examples
/// #### Subscription Ack
/// ```json
/// {"id":"1","type":"ack"}
/// ```
/// #### Subscription Error
/// ```json
/// {"id":"1","type":"error","code":"400"}
/// ```
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum KucoinSubscription {
    #[serde(rename = "ack")]
    Ack { id: String },
    Error { id: String, code: String },
}

impl Validator for KucoinSubscription {
    fn validate(self) -> Result<Self, SocketError>
    where
        Self: Sized,
    {
        match self {
            Self::Ack { .. } => Ok(self),
            Self::Error { code, .. } => Err(SocketError::Subscribe(format!(
                "received failure subscription response code: {code}",
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_kucoin_subscription() {
        let ok: KucoinSubscription = serde_json::from_str("{\"id\":\"1\",\"type\":\"ack\"}").unwrap();
        assert!(ok.clone().validate().is_ok());

        let err: KucoinSubscription = serde_json::from_str("{\"id\":\"1\",\"type\":\"error\",\"code\":\"400\"}").unwrap();
        assert!(err.validate().is_err());
    }
}

