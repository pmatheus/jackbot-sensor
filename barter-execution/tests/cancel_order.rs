use barter_execution::{
    exchange::mock::MockExchange,
    client::mock::MockExecutionConfig,
    balance::{AssetBalance, Balance},
    order::{
        Order, OrderKey, OrderKind, TimeInForce,
        id::{ClientOrderId, OrderId, StrategyId},
        request::{OrderRequestCancel, RequestCancel},
        state::{Open},
    },
    UnindexedAccountSnapshot, InstrumentAccountSnapshot,
};
use barter_instrument::{
    Side,
    asset::{name::AssetNameExchange},
    exchange::ExchangeId,
    test_utils::instrument as test_instrument,
};
use chrono::Utc;
use fnv::FnvHashMap;
use rust_decimal::Decimal;
use tokio::sync::{broadcast, mpsc};

fn build_exchange() -> MockExchange {
    let exchange = ExchangeId::Mock;
    let instrument = test_instrument(exchange, "btc", "usdt");
    let mut instruments = FnvHashMap::default();
    instruments.insert(instrument.name_exchange.clone(), instrument.clone());

    let cid = ClientOrderId::new("cid1");
    let open_order = Order {
        key: OrderKey {
            exchange,
            instrument: instrument.name_exchange.clone(),
            strategy: StrategyId::new("strat"),
            cid: cid.clone(),
        },
        side: Side::Buy,
        price: Decimal::ONE,
        quantity: Decimal::ONE,
        kind: OrderKind::Market,
        time_in_force: TimeInForce::ImmediateOrCancel,
        state: Open {
            id: OrderId::new("id1"),
            time_exchange: Utc::now(),
            filled_quantity: Decimal::ZERO,
        },
    };

    let snapshot = UnindexedAccountSnapshot {
        exchange,
        balances: vec![AssetBalance {
            asset: AssetNameExchange::from("usdt"),
            balance: Balance { total: Decimal::ONE, free: Decimal::ONE },
            time_exchange: Utc::now(),
        }],
        instruments: vec![InstrumentAccountSnapshot {
            instrument: instrument.name_exchange.clone(),
            orders: vec![open_order.into()],
        }],
    };

    let (_tx, rx) = mpsc::unbounded_channel();
    let (event_tx, _event_rx) = broadcast::channel(16);

    MockExchange::new(
        MockExecutionConfig {
            mocked_exchange: exchange,
            initial_state: snapshot,
            latency_ms: 0,
            fees_percent: Decimal::ZERO,
        },
        rx,
        event_tx,
        instruments,
    )
}

#[test]
fn test_cancel_order_success_and_fail() {
    let mut exchange = build_exchange();
    let instrument_key = exchange.instruments.keys().next().unwrap().clone();
    let cid = ClientOrderId::new("cid1");
    let request = OrderRequestCancel {
        key: OrderKey {
            exchange: ExchangeId::Mock,
            instrument: instrument_key.clone(),
            strategy: StrategyId::new("strat"),
            cid: cid.clone(),
        },
        state: RequestCancel { id: None },
    };

    let first = exchange.cancel_order(request.clone());
    assert!(first.state.is_ok());
    assert_eq!(exchange.account.orders_open().count(), 0);

    let second = exchange.cancel_order(request);
    assert!(second.state.is_err());
}
