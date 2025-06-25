use jackbot_data::books::l2_sequencer::{BinanceSpotOrderBookL2Sequencer, HasUpdateIds, L2Sequencer};
use jackbot_data::error::DataError;

#[derive(Clone)]
struct DummyUpdate {
    first: u64,
    last: u64,
}

impl HasUpdateIds for DummyUpdate {
    fn first_update_id(&self) -> u64 {
        self.first
    }
    fn last_update_id(&self) -> u64 {
        self.last
    }
}

#[test]
fn test_sequencer_valid_flow() {
    let mut seq = <BinanceSpotOrderBookL2Sequencer as L2Sequencer<DummyUpdate>>::new(100);
    // first valid update
    let up1 = DummyUpdate {
        first: 101,
        last: 102,
    };
    assert!(
        <BinanceSpotOrderBookL2Sequencer as L2Sequencer<DummyUpdate>>::validate_sequence(
            &mut seq, up1
        )
        .unwrap()
        .is_some()
    );
    assert!(!<BinanceSpotOrderBookL2Sequencer as L2Sequencer<
        DummyUpdate,
    >>::is_first_update(&seq));

    // next valid update must start from last id + 1
    let up2 = DummyUpdate {
        first: 103,
        last: 105,
    };
    assert!(
        <BinanceSpotOrderBookL2Sequencer as L2Sequencer<DummyUpdate>>::validate_sequence(
            &mut seq, up2
        )
        .is_ok()
    );
    assert_eq!(seq.last_update_id, 105);
}

#[test]
fn test_sequencer_invalid_first() {
    let mut seq = <BinanceSpotOrderBookL2Sequencer as L2Sequencer<DummyUpdate>>::new(100);
    let bad = DummyUpdate {
        first: 105,
        last: 106,
    };
    assert!(matches!(
        <BinanceSpotOrderBookL2Sequencer as L2Sequencer<DummyUpdate>>::validate_sequence(
            &mut seq, bad
        ),
        Err(DataError::InvalidSequence { .. })
    ));
}

#[test]
fn test_sequencer_invalid_next() {
    let mut seq = <BinanceSpotOrderBookL2Sequencer as L2Sequencer<DummyUpdate>>::new(100);
    let good = DummyUpdate {
        first: 101,
        last: 103,
    };
    <BinanceSpotOrderBookL2Sequencer as L2Sequencer<DummyUpdate>>::validate_sequence(
        &mut seq, good,
    )
    .unwrap();
    let bad = DummyUpdate {
        first: 105,
        last: 106,
    };
    assert!(matches!(
        <BinanceSpotOrderBookL2Sequencer as L2Sequencer<DummyUpdate>>::validate_sequence(
            &mut seq, bad
        ),
        Err(DataError::InvalidSequence { .. })
    ));
}
