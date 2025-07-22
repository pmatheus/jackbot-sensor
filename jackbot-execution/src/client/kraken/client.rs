use crate::{
    balance::AssetBalance,
    client::ExecutionClient,
    error::{UnindexedClientError, UnindexedOrderError},
    order::{
        id::{ClientOrderId, OrderId},
        request::{OrderRequestCancel, OrderRequestOpen, UnindexedOrderResponseCancel},
        state::{Cancelled, Open},
        Order,
    },
    trade::Trade,
    UnindexedAccountEvent, UnindexedAccountSnapshot,
};
use chrono::{DateTime, Utc};
use jackbot_instrument::{
    asset::{name::AssetNameExchange, QuoteAsset},
    exchange::ExchangeId,
    instrument::name::InstrumentNameExchange,
};
use tokio_stream::wrappers::UnboundedReceiverStream;

use super::{
    rest::{KrakenRestClient, KrakenRestConfig},
    websocket::{KrakenWsClient, KrakenWsConfig},
};

/// Comprehensive Kraken exchange client combining REST and WebSocket functionality
#[derive(Clone)]
pub struct KrakenClient {
    rest_client: KrakenRestClient,
    ws_client: KrakenWsClient,
}

impl KrakenClient {
    pub fn new(rest_config: KrakenRestConfig, ws_config: KrakenWsConfig) -> Self {
        Self {
            rest_client: KrakenRestClient::new(rest_config),
            ws_client: KrakenWsClient::new(ws_config),
        }
    }

    /// Create a new client with default WebSocket configuration
    pub fn with_rest_config(rest_config: KrakenRestConfig) -> Self {
        Self {
            rest_client: KrakenRestClient::new(rest_config),
            ws_client: KrakenWsClient::new(KrakenWsConfig::default()),
        }
    }

    /// Get REST client for direct access to REST API methods
    pub fn rest(&self) -> &KrakenRestClient {
        &self.rest_client
    }

    /// Get WebSocket client for direct access to WebSocket methods
    pub fn websocket(&self) -> &KrakenWsClient {
        &self.ws_client
    }
}

impl ExecutionClient for KrakenClient {
    const EXCHANGE: ExchangeId = ExchangeId::Kraken;
    type Config = KrakenClientConfig;
    type AccountStream = UnboundedReceiverStream<UnindexedAccountEvent>;

    fn new(config: Self::Config) -> Self {
        Self::new(config.rest, config.websocket)
    }

    async fn account_snapshot(
        &self,
        assets: &[AssetNameExchange],
        instruments: &[InstrumentNameExchange],
    ) -> Result<UnindexedAccountSnapshot, UnindexedClientError> {
        // Fetch balances via REST API
        let balances = self.rest_client.fetch_balances().await?;
        
        // Fetch open orders via REST API  
        let orders = self.rest_client.fetch_open_orders().await?;
        
        // Convert orders to instruments for snapshot
        let instruments_with_orders: Vec<_> = orders.into_iter()
            .map(|order| order.key.instrument)
            .collect();

        Ok(UnindexedAccountSnapshot {
            exchange: Self::EXCHANGE,
            balances,
            instruments: instruments_with_orders,
        })
    }

    async fn account_stream(
        &self,
        assets: &[AssetNameExchange],
        instruments: &[InstrumentNameExchange],
    ) -> Result<Self::AccountStream, UnindexedClientError> {
        self.ws_client.account_stream(assets, instruments).await
    }

    async fn cancel_order(
        &self,
        request: OrderRequestCancel<ExchangeId, InstrumentNameExchange>,
    ) -> UnindexedOrderResponseCancel {
        match self.rest_client.cancel_order(request.clone()).await {
            Ok(cancelled) => UnindexedOrderResponseCancel {
                key: request.key,
                state: Ok(cancelled),
            },
            Err(error) => UnindexedOrderResponseCancel {
                key: request.key,
                state: Err(error),
            },
        }
    }

    async fn open_order(
        &self,
        request: OrderRequestOpen<ExchangeId, InstrumentNameExchange>,
    ) -> Order<ExchangeId, InstrumentNameExchange, Result<Open, UnindexedOrderError>> {
        match self.rest_client.place_order(request.clone()).await {
            Ok(order) => Order {
                key: order.key,
                side: order.side,
                price: order.price,
                quantity: order.quantity,
                kind: order.kind,
                time_in_force: order.time_in_force,
                state: Ok(order.state),
            },
            Err(error) => Order {
                key: request.key,
                side: request.state.side,
                price: request.state.price,
                quantity: request.state.quantity,
                kind: request.state.kind,
                time_in_force: request.state.time_in_force,
                state: Err(error),
            },
        }
    }

    async fn fetch_balances(
        &self,
    ) -> Result<Vec<AssetBalance<AssetNameExchange>>, UnindexedClientError> {
        self.rest_client.fetch_balances().await
    }

    async fn fetch_open_orders(
        &self,
    ) -> Result<Vec<Order<ExchangeId, InstrumentNameExchange, Open>>, UnindexedClientError> {
        self.rest_client.fetch_open_orders().await
    }

    async fn fetch_trades(
        &self,
        time_since: DateTime<Utc>,
    ) -> Result<Vec<Trade<QuoteAsset, InstrumentNameExchange>>, UnindexedClientError> {
        self.rest_client.fetch_trades(time_since).await
    }
}

/// Configuration for KrakenClient combining REST and WebSocket configs
#[derive(Clone, Debug)]
pub struct KrakenClientConfig {
    pub rest: KrakenRestConfig,
    pub websocket: KrakenWsConfig,
}

impl KrakenClientConfig {
    pub fn new(rest: KrakenRestConfig, websocket: KrakenWsConfig) -> Self {
        Self { rest, websocket }
    }

    /// Create configuration with REST config and default WebSocket config
    pub fn with_rest(rest: KrakenRestConfig) -> Self {
        Self {
            rest,
            websocket: KrakenWsConfig::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::kraken::rest::KrakenTier;

    #[test]
    fn test_kraken_client_creation() {
        let rest_config = KrakenRestConfig {
            api_key: "test_key".to_string(),
            api_secret: "test_secret".to_string(),
            tier: KrakenTier::Starter,
            sandbox: true,
        };

        let ws_config = KrakenWsConfig::default();
        let client_config = KrakenClientConfig::new(rest_config, ws_config);
        let _client = KrakenClient::new(client_config.rest, client_config.websocket);
    }

    #[test]
    fn test_kraken_client_with_rest_only() {
        let rest_config = KrakenRestConfig {
            api_key: "test_key".to_string(),
            api_secret: "test_secret".to_string(),
            tier: KrakenTier::Pro,
            sandbox: false,
        };

        let _client = KrakenClient::with_rest_config(rest_config);
    }
}