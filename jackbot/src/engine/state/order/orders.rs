use derive_more::Constructor;
use fnv::FnvHashMap;
use jackbot_execution::order::{
    Order,
    id::ClientOrderId,
    state::ActiveOrderState,
};
use jackbot_instrument::{exchange::ExchangeIndex, instrument::InstrumentIndex};
use serde::{Deserialize, Serialize};

/// Synchronous order manager that tracks the lifecycle of active exchange orders.
///
/// The `Orders` struct maintains a `FnvHashMap` of orders keyed by their [`ClientOrderId`].
///
/// Implements the [`OrderManager`](super::manager::OrderManager) and 
/// [`InFlightRequestRecorder`](super::in_flight_recorder::InFlightRequestRecorder) traits.
///
/// A distinct instance of `Orders` is used in the engine
/// [`InstrumentState`](crate::engine::state::instrument::InstrumentState) to track the active orders for
/// each instrument, however it could be used to track global orders if [`ClientOrderId`]
/// is globally unique.
///
/// # State Transitions
/// Orders tend to progress through the following states:
/// 1. OpenInFlight - Initial order request sent to exchange
/// 2. Open - Order confirmed as open on exchange
/// 3. CancelInFlight - Cancellation request sent to exchange
/// 4. Cancelled/Expired/FullyFilled - Terminal states, once achieved order is no longer tracked.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, Constructor)]
pub struct Orders<ExchangeKey = ExchangeIndex, InstrumentKey = InstrumentIndex>(
    pub FnvHashMap<ClientOrderId, Order<ExchangeKey, InstrumentKey, ActiveOrderState>>,
);

impl<ExchangeKey, InstrumentKey> Default for Orders<ExchangeKey, InstrumentKey> {
    fn default() -> Self {
        Self(FnvHashMap::default())
    }
}