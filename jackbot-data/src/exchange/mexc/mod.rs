//! MEXC exchange module.

/// Spot market modules for MEXC.
pub mod spot;
/// Futures market modules for MEXC.
pub mod futures;
/// Trade event types for MEXC.
pub mod trade;
/// Rate limiting utilities for MEXC.
pub mod rate_limit;
/// Channel types for MEXC.
pub mod channel;
/// Market types for MEXC.
pub mod market;
/// Subscription response types for MEXC.
pub mod subscription;

use self::{channel::MexcChannel, market::MexcMarket, subscription::MexcSubResponse, trade::MexcTrades};
use crate::{
    ExchangeWsStream, NoInitialSnapshots,
    exchange::{Connector, ExchangeSub, PingInterval, StreamSelector, DEFAULT_PING_INTERVAL, DEFAULT_HEARTBEAT_INTERVAL},
    instrument::InstrumentData,
    subscriber::{WebSocketSubscriber, validator::WebSocketSubValidator},
    subscription::trade::PublicTrades,
    transformer::stateless::StatelessTransformer,
};
use jackbot_instrument::exchange::ExchangeId;
use jackbot_integration::{error::SocketError, protocol::websocket::WsMessage};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::time::Duration;
use url::Url;

/// MEXC Spot WebSocket base URL.
pub const BASE_URL_MEXC_SPOT: &str = "wss://wbs.mexc.com/ws";

#[derive(Clone, Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mexc;

impl Connector for Mexc {
    const ID: ExchangeId = ExchangeId::Mexc;
    type Channel = MexcChannel;
    type Market = MexcMarket;
    type Subscriber = WebSocketSubscriber;
    type SubValidator = WebSocketSubValidator;
    type SubResponse = MexcSubResponse;

    fn url() -> Result<Url, SocketError> {
        Url::parse(BASE_URL_MEXC_SPOT).map_err(SocketError::UrlParse)
    }

    fn ping_interval() -> Option<PingInterval> {
        Some(PingInterval {
            interval: tokio::time::interval(DEFAULT_PING_INTERVAL),
            ping: || WsMessage::text("ping"),
        })
    }

    fn requests(exchange_subs: Vec<ExchangeSub<Self::Channel, Self::Market>>) -> Vec<WsMessage> {
        let params = exchange_subs
            .into_iter()
            .map(|sub| format!("{}@{}", sub.channel.as_ref(), sub.market.as_ref()))
            .collect::<Vec<String>>();

        vec![WsMessage::text(
            json!({
                "method": "SUBSCRIPTION",
                "params": params
            })
            .to_string(),
        )]
    }

    fn heartbeat_interval() -> Option<Duration> {
        Some(DEFAULT_HEARTBEAT_INTERVAL)
    }
}

impl<Instrument> StreamSelector<Instrument, PublicTrades> for Mexc
where
    Instrument: InstrumentData,
{
    type SnapFetcher = NoInitialSnapshots;
    type Stream = ExchangeWsStream<StatelessTransformer<Self, Instrument::Key, PublicTrades, MexcTrades>>;
}
