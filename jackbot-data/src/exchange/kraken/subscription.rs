use super::message::KrakenError;
use jackbot_integration::{Validator, error::SocketError};
use serde::{Deserialize, Serialize};

/// [`Kraken`](super::Kraken) message received in response to WebSocket subscription requests.
///
/// ### Raw Payload Examples
/// See docs: <https://docs.kraken.com/websockets/#message-subscriptionStatus>
/// #### Subscription Trade Success
/// ```json
/// {
///   "channelID": 10001,
///   "channelName": "ticker",
///   "event": "subscriptionStatus",
///   "pair": "XBT/EUR",
///   "status": "subscribed",
///   "subscription": {
///     "name": "ticker"
///   }
/// }
/// ```
///
/// #### Subscription Trade Failure
/// ```json
/// {
///   "errorMessage": "Subscription name invalid",
///   "event": "subscriptionStatus",
///   "pair": "XBT/USD",
///   "status": "error",
///   "subscription": {
///     "name": "trades"
///   }
/// }
/// ```
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Deserialize, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum KrakenSubResponse {
    Subscribed {
        #[serde(alias = "channelID")]
        channel_id: u64,
        #[serde(alias = "channelName")]
        channel_name: String,
        pair: String,
    },
    Error(KrakenError),
}

impl Validator for KrakenSubResponse {
    type Item = KrakenSubResponse;

    fn validate(&self, item: &Self::Item) -> Result<(), SocketError> {
        match item {
            KrakenSubResponse::Subscribed { .. } => Ok(()),
            KrakenSubResponse::Error(error) => Err(SocketError::Subscribe(format!(
                "received failure subscription response: {}",
                error.message
            ))),
        }
    }
}
