use Jackbot::backtest::order_book_replay::OrderBookReplay;
use jackbot_data::{books::{OrderBook}, subscription::book::OrderBookEvent};
use rust_decimal_macros::dec;

#[test]
fn test_order_book_replay_updates_book() {
    // initial snapshot with one bid and ask
    let snapshot = OrderBook::new(
        0,
        None,
        vec![(dec!(100), dec!(1))],
        vec![(dec!(101), dec!(1))],
    );

    let mut replay = OrderBookReplay::new(snapshot);

    // update event replaces levels
    let update = OrderBook::new(
        1,
        None,
        vec![(dec!(102), dec!(2))],
        vec![(dec!(103), dec!(2))],
    );
    replay.apply(OrderBookEvent::Update(update));

    let book = replay.current();
    assert_eq!(book.bids().levels()[0].price, dec!(102));
    assert_eq!(book.asks().levels()[0].price, dec!(103));
}
