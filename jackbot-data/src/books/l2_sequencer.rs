use crate::error::DataError;

/// Trait for L2 order book sequencing logic.
///
/// Exchanges that provide incremental order book updates usually include
/// sequence numbers that allow clients to ensure no updates were missed.
/// Implementations of this trait encapsulate the exchange specific rules for
/// validating those sequences. Each call to [`validate_sequence`] consumes an
/// update and returns it back if the update should be applied to the local
/// order book. If an update is outdated it can be dropped by returning `Ok(None)`
/// and any sequencing error should return [`DataError::InvalidSequence`].
pub trait L2Sequencer<Update>: std::fmt::Debug + Send + Sync {
    /// Create a new sequencer from the initial snapshot sequence.
    fn new(last_update_id: u64) -> Self
    where
        Self: Sized;
    /// Validate and process an incoming update. Returns Some(valid_update) if the update should be applied, None if it should be dropped, or Err if there is a sequencing error.
    fn validate_sequence(&mut self, update: Update) -> Result<Option<Update>, DataError>;
    /// Returns true if this is the first update after the snapshot.
    fn is_first_update(&self) -> bool;
}

/// Example implementation for Binance Spot order books.
///
/// The Binance Spot WebSocket feed exposes `U` (first update id) and `u`
/// (last update id) fields. Updates must start from the snapshot `lastUpdateId + 1`
/// and thereafter each update must begin where the previous ended. This
/// sequencer enforces those rules.
#[derive(Debug, Clone)]
pub struct BinanceSpotOrderBookL2Sequencer {
    pub updates_processed: u64,
    pub last_update_id: u64,
    pub prev_last_update_id: u64,
}

impl BinanceSpotOrderBookL2Sequencer {
    pub fn validate_first_update<Update: HasUpdateIds>(
        &self,
        update: &Update,
    ) -> Result<(), DataError> {
        // Binance Spot step 5:
        // "The first processed event should have U <= lastUpdateId+1 AND u >= lastUpdateId+1"
        if update.first_update_id() <= self.last_update_id + 1
            && update.last_update_id() >= self.last_update_id + 1
        {
            Ok(())
        } else {
            Err(DataError::InvalidSequence {
                prev_last_update_id: self.last_update_id,
                first_update_id: update.first_update_id(),
            })
        }
    }
    pub fn validate_next_update<Update: HasUpdateIds>(
        &self,
        update: &Update,
    ) -> Result<(), DataError> {
        // Binance Spot step 6:
        // "Each new event's U should be equal to the previous event's u+1"
        if update.first_update_id() == self.prev_last_update_id + 1 {
            Ok(())
        } else {
            Err(DataError::InvalidSequence {
                prev_last_update_id: self.prev_last_update_id,
                first_update_id: update.first_update_id(),
            })
        }
    }
}

impl<Update: HasUpdateIds> L2Sequencer<Update> for BinanceSpotOrderBookL2Sequencer {
    fn new(last_update_id: u64) -> Self {
        Self {
            updates_processed: 0,
            last_update_id,
            prev_last_update_id: last_update_id,
        }
    }
    fn validate_sequence(&mut self, update: Update) -> Result<Option<Update>, DataError> {
        if self.updates_processed == 0 {
            self.validate_first_update(&update)?;
        } else {
            self.validate_next_update(&update)?;
        }
        self.prev_last_update_id = self.last_update_id;
        self.last_update_id = update.last_update_id();
        self.updates_processed += 1;
        Ok(Some(update))
    }
    fn is_first_update(&self) -> bool {
        self.updates_processed == 0
    }
}

pub trait HasUpdateIds {
    fn first_update_id(&self) -> u64;
    fn last_update_id(&self) -> u64;
}

// Example: implement HasUpdateIds for BinanceSpotOrderBookL2Update
// (The actual struct is in binance/spot/l2.rs, so this is just a trait definition for now)

#[cfg(test)]
mod tests {
    use super::*;

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
        let mut seq = BinanceSpotOrderBookL2Sequencer::new(100);
        // first valid update
        let up1 = DummyUpdate { first: 101, last: 102 };
        assert!(seq.validate_sequence(up1).unwrap().is_some());
        assert!(!seq.is_first_update());

        // next valid update must start from last id + 1
        let up2 = DummyUpdate { first: 103, last: 105 };
        assert!(seq.validate_sequence(up2).is_ok());
        assert_eq!(seq.last_update_id, 105);
    }

    #[test]
    fn test_sequencer_invalid_first() {
        let mut seq = BinanceSpotOrderBookL2Sequencer::new(100);
        let bad = DummyUpdate { first: 105, last: 106 };
        assert!(matches!(
            seq.validate_sequence(bad),
            Err(DataError::InvalidSequence { .. })
        ));
    }

    #[test]
    fn test_sequencer_invalid_next() {
        let mut seq = BinanceSpotOrderBookL2Sequencer::new(100);
        let good = DummyUpdate { first: 101, last: 103 };
        seq.validate_sequence(good).unwrap();
        let bad = DummyUpdate { first: 105, last: 106 };
        assert!(matches!(
            seq.validate_sequence(bad),
            Err(DataError::InvalidSequence { .. })
        ));
    }
}
