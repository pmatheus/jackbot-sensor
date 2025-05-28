use jackbot_execution::{
    client::ExecutionClient,
    client::binance::{BinanceWsClient, BinanceWsConfig},
    order::{
        OrderKey, OrderKind, TimeInForce,
        id::{ClientOrderId, StrategyId},
        request::{OrderRequestOpen, RequestOpen},
    },
    trading::TradingClient,
};
use jackbot_instrument::{Side, exchange::ExchangeId, instrument::name::InstrumentNameExchange};
use rust_decimal_macros::dec;
use url::Url;

#[tokio::test]
async fn binance_open_order_stub() {
    let client = BinanceWsClient::new(BinanceWsConfig {
        url: Url::parse("ws://localhost").unwrap(),
        auth_payload: "{}".to_string(),
    });
    let request = OrderRequestOpen {
        key: OrderKey {
            exchange: ExchangeId::BinanceSpot,
            instrument: InstrumentNameExchange::from("BTCUSDT"),
            strategy: StrategyId::unknown(),
            cid: ClientOrderId::default(),
        },
        state: RequestOpen {
            side: Side::Buy,
            price: dec!(0),
            quantity: dec!(1),
            kind: OrderKind::Market,
            time_in_force: TimeInForce::GoodUntilCancelled { post_only: false },
        },
    };

    let order = jackbot_execution::client::ExecutionClient::open_order(&client, request).await;
    assert!(order.state.is_ok());
}
