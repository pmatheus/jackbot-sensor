use jackbot_execution::{
    client::binance::paper::{BinancePaperClient, BinancePaperConfig},
    exchange::paper::PaperBook,
    order::{
        id::{ClientOrderId, StrategyId},
        request::{OrderRequestOpen, RequestOpen},
        OrderKey, OrderKind, TimeInForce,
    },
    UnindexedAccountSnapshot,
};
use jackbot_instrument::{
    exchange::ExchangeId,
    instrument::{Instrument, name::InstrumentNameExchange},
    asset::name::AssetNameExchange,
    Underlying,
    Side,
};
use fnv::FnvHashMap;
use rust_decimal_macros::dec;

#[tokio::test]
async fn test_binance_paper_client_open_order() {
    let instrument = Instrument::spot(
        ExchangeId::BinanceSpot,
        "btc_usdt",
        "BTC-USDT",
        Underlying::new("btc", "usdt"),
        None,
    );
    let mut instruments = FnvHashMap::default();
    instruments.insert(instrument.name_exchange.clone(), instrument);

    let book = PaperBook::new(vec![(dec!(99), dec!(1))], vec![(dec!(101), dec!(1))]);
    let mut books = FnvHashMap::default();
    books.insert(InstrumentNameExchange::from("BTC-USDT"), book);

    let snapshot = UnindexedAccountSnapshot {
        exchange: ExchangeId::BinanceSpot,
        balances: vec![jackbot_execution::balance::AssetBalance::new(
            AssetNameExchange::from("usdt"),
            jackbot_execution::balance::Balance::new(dec!(1000), dec!(1000)),
            chrono::Utc::now(),
        )],
        instruments: Vec::new(),
    };

    let config = BinancePaperConfig {
        books,
        instruments,
        snapshot,
        fees_percent: dec!(0),
    };
    let client = BinancePaperClient::new(config);

    let request = OrderRequestOpen {
        key: OrderKey {
            exchange: ExchangeId::BinanceSpot,
            instrument: InstrumentNameExchange::from("BTC-USDT"),
            strategy: StrategyId::new("s"),
            cid: ClientOrderId::new("1"),
        },
        state: RequestOpen {
            side: Side::Buy,
            price: dec!(0),
            quantity: dec!(1),
            kind: OrderKind::Market,
            time_in_force: TimeInForce::ImmediateOrCancel,
        },
    };

    let order = client.open_order(request).await;
    assert!(order.state.is_ok());
}
