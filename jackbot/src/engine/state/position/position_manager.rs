//! Position manager for handling position lifecycle.

use derive_more::Constructor;
use jackbot_execution::trade::Trade;
use jackbot_instrument::{
    asset::QuoteAsset,
    instrument::InstrumentIndex,
};
use serde::{Deserialize, Serialize};
use std::fmt::Debug;

use crate::engine::state::position::{Position, PositionExited};

#[derive(Debug, Clone, PartialEq, PartialOrd, Deserialize, Serialize, Constructor)]
pub struct PositionManager<InstrumentKey = InstrumentIndex> {
    pub current: Option<Position<QuoteAsset, InstrumentKey>>,
}

impl<InstrumentKey> Default for PositionManager<InstrumentKey> {
    fn default() -> Self {
        Self { current: None }
    }
}

impl<InstrumentKey> PositionManager<InstrumentKey> {
    /// Updates the current position state based on a new trade.
    ///
    /// This method handles:
    /// - Opening a new position if none exists
    /// - Updating an existing position (increase/decrease/close)
    /// - Handling position flips (close existing & open new with any remaining trade quantity)
    pub fn update_from_trade(
        &mut self,
        trade: &Trade<QuoteAsset, InstrumentKey>,
    ) -> Option<PositionExited<QuoteAsset, InstrumentKey>>
    where
        InstrumentKey: Debug + Clone + PartialEq,
    {
        let (current, closed) = match self.current.take() {
            Some(position) => {
                // Update current Position, maybe closing it, and maybe opening a new Position
                // with leftover trade.quantity
                position.update_from_trade(trade)
            }
            None => {
                // No current Position, so enter a new one with Trade
                (Some(Position::from(trade)), None)
            }
        };

        self.current = current;

        closed
    }
}