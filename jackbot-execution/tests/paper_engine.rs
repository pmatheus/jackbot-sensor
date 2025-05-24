use jackbot_execution::{
    exchange::paper::{PaperBook, PaperEngine},
    order::{id::{ClientOrderId, StrategyId}, request::{OrderRequestOpen, RequestOpen}, OrderKey, OrderKind, TimeInForce},
    UnindexedAccountSnapshot,
};
use jackbot_instrument::{exchange::ExchangeId, instrument::Instrument, instrument::name::InstrumentNameExchange, Underlying, asset::name::AssetNameExchange, Side};
use fnv::FnvHashMap;
use rust_decimal_macros::dec;

#[test]
fn test_paper_engine_market_fill() {
    // setup instrument and orderbook
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

    // account with 1000 usdt
    let snapshot = UnindexedAccountSnapshot {
        exchange: ExchangeId::BinanceSpot,
        balances: vec![jackbot_execution::balance::AssetBalance::new(
            AssetNameExchange::from("usdt"),
            jackbot_execution::balance::Balance::new(dec!(1000), dec!(1000)),
            chrono::Utc::now(),
        )],
        instruments: Vec::new(),
    };

    let mut engine = PaperEngine::new(ExchangeId::BinanceSpot, dec!(0), instruments, books, snapshot);

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

    let (order, notifications) = engine.open_order(request);
    assert!(notifications.is_some());
    let n = notifications.unwrap();
    assert_eq!(order.price, dec!(101));
    assert_eq!(n.trade.price, dec!(101));
    assert_eq!(n.trade.quantity, dec!(1));
}
