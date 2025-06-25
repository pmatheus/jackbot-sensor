use jackbot_data::books::aggregator::*;
use jackbot_data::books::{Level, OrderBook};
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
fn detects_simple_arbitrage() {
    let book_a = build_book(dec!(10), dec!(11));
    let book_b = build_book(dec!(12), dec!(13));

    let agg = OrderBookAggregator::new([
        ExchangeBook {
            exchange: ExchangeId::BinanceSpot,
            book: book_a,
            weight: Decimal::ONE,
        },
        ExchangeBook {
            exchange: ExchangeId::Coinbase,
            book: book_b,
            weight: Decimal::ONE,
        },
    ]);

    let opp = agg.detect_arbitrage(dec!(0)).expect("should detect");
    assert_eq!(opp.buy_exchange, ExchangeId::BinanceSpot);
    assert_eq!(opp.sell_exchange, ExchangeId::Coinbase);
    assert_eq!(opp.buy_price, dec!(11));
    assert_eq!(opp.sell_price, dec!(12));
}

#[test]
fn no_arbitrage_below_threshold() {
    let book_a = build_book(dec!(10), dec!(11));
    let book_b = build_book(dec!(11.4), dec!(12));

    let agg = OrderBookAggregator::new([
        ExchangeBook {
            exchange: ExchangeId::BinanceSpot,
            book: book_a,
            weight: Decimal::ONE,
        },
        ExchangeBook {
            exchange: ExchangeId::Coinbase,
            book: book_b,
            weight: Decimal::ONE,
        },
    ]);

    assert!(agg.detect_arbitrage(dec!(0.5)).is_none());
}

#[test]
fn aggregates_books_by_weight() {
    let book_a = build_book(dec!(10), dec!(11));
    let book_b = build_book(dec!(12), dec!(13));

    let agg = OrderBookAggregator::new([
        ExchangeBook {
            exchange: ExchangeId::BinanceSpot,
            book: book_a,
            weight: dec!(2),
        },
        ExchangeBook {
            exchange: ExchangeId::Coinbase,
            book: book_b,
            weight: Decimal::ONE,
        },
    ]);

    let merged = agg.aggregate(1);
    assert_eq!(merged.bids().levels()[0].amount, dec!(3));
    assert_eq!(merged.asks().levels()[0].amount, dec!(3));
}

#[test]
fn detects_arbitrage_with_three_exchanges() {
    let book_a = build_book(dec!(9), dec!(10));
    let book_b = build_book(dec!(11), dec!(12));
    let book_c = build_book(dec!(13), dec!(14));

    let agg = OrderBookAggregator::new([
        ExchangeBook {
            exchange: ExchangeId::BinanceSpot,
            book: book_a,
            weight: Decimal::ONE,
        },
        ExchangeBook {
            exchange: ExchangeId::Coinbase,
            book: book_b,
            weight: Decimal::ONE,
        },
        ExchangeBook {
            exchange: ExchangeId::Kraken,
            book: book_c,
            weight: Decimal::ONE,
        },
    ]);

    let opp = agg.detect_arbitrage(dec!(0)).expect("should detect");
    assert_eq!(opp.buy_exchange, ExchangeId::BinanceSpot);
    assert_eq!(opp.sell_exchange, ExchangeId::Kraken);
    assert_eq!(opp.spread, dec!(3));
}
