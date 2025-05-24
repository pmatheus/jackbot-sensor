use jackbot_strategy::{Strategy, StrategyConfig, arbitrage::ArbitrageStrategy};
use jackbot_data::books::{aggregator::{OrderBookAggregator, ExchangeBook}, OrderBook, Level};
use jackbot_instrument::exchange::ExchangeId;
use parking_lot::RwLock;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::sync::Arc;

fn build_book(bid: Decimal, ask: Decimal) -> Arc<RwLock<OrderBook>> {
    Arc::new(RwLock::new(OrderBook::new(
        0,
        None,
        vec![Level::new(bid, dec!(1))],
        vec![Level::new(ask, dec!(1))],
    )))
}

#[test]
fn strategy_detects_and_records_opportunity() {
    let book_a = build_book(dec!(10), dec!(11));
    let book_b = build_book(dec!(12), dec!(13));

    let agg = OrderBookAggregator::new([
        ExchangeBook { exchange: ExchangeId::BinanceSpot, book: book_a, weight: Decimal::ONE },
        ExchangeBook { exchange: ExchangeId::Coinbase, book: book_b, weight: Decimal::ONE },
    ]);

    let config = StrategyConfig { parameters: [ ("threshold".into(), 0.0) ].into_iter().collect() };
    let mut strat = ArbitrageStrategy::new(agg);
    strat.on_start(&config);
    strat.on_event(&());

    assert_eq!(strat.metrics.opportunities_detected, 1);
    assert_eq!(strat.metrics.opportunities_executed, 1);
}

#[test]
fn strategy_respects_threshold() {
    let book_a = build_book(dec!(10), dec!(11));
    let book_b = build_book(dec!(11.4), dec!(12));

    let agg = OrderBookAggregator::new([
        ExchangeBook { exchange: ExchangeId::BinanceSpot, book: book_a, weight: Decimal::ONE },
        ExchangeBook { exchange: ExchangeId::Coinbase, book: book_b, weight: Decimal::ONE },
    ]);

    let config = StrategyConfig { parameters: [ ("threshold".into(), 0.5) ].into_iter().collect() };
    let mut strat = ArbitrageStrategy::new(agg);
    strat.on_start(&config);
    strat.on_event(&());

    assert_eq!(strat.metrics.opportunities_detected, 0);
}
