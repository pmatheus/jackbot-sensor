//! Subscription logic and types for Binance exchange.
use jackbot_integration::{Validator, error::SocketError};
use serde::{Deserialize, Serialize};

/// [`Binance`](super::Binance) subscription response message.
///
/// ### Raw Payload Examples
/// See docs: <https://binance-docs.github.io/apidocs/spot/en/#live-subscribing-unsubscribing-to-streams>
/// #### Subscription Success
/// ```json
/// {
///     "id":1,
///     "result":null
/// }
/// ```
///
/// #### Subscription Failure
/// ```json
/// {
///     "id":1,
///     "result":[]
/// }
/// ```
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Deserialize, Serialize)]
pub struct BinanceSubResponse {
    result: Option<Vec<String>>,
    id: u32,
}

impl Validator for BinanceSubResponse {
    type Item = BinanceSubResponse;

    fn validate(&self, item: &Self::Item) -> Result<(), SocketError> {
        if item.result.is_none() {
            Ok(())
        } else {
            Err(SocketError::Subscribe(
                "received failure subscription response".to_owned(),
            ))
        }
    }
}
