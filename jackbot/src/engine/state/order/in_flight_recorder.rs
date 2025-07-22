use crate::engine::state::{EngineState, order::orders::Orders};
use jackbot_execution::order::{
    Order,
    request::{OrderRequestCancel, OrderRequestOpen},
    state::{ActiveOrderState, CancelInFlight},
};
use jackbot_instrument::{exchange::ExchangeIndex, instrument::InstrumentIndex};
use std::fmt::Debug;
use tracing::error;

/// Synchronous in-flight open and in-flight cancel order request tracker.
///
/// See [`Orders`](super::Orders) for an example implementation.
pub trait InFlightRequestRecorder<ExchangeKey = ExchangeIndex, InstrumentKey = InstrumentIndex> {
    fn record_in_flight_cancels<'a>(
        &mut self,
        requests: impl IntoIterator<Item = &'a OrderRequestCancel<ExchangeKey, InstrumentKey>>,
    ) where
        ExchangeKey: 'a,
        InstrumentKey: 'a,
    {
        requests
            .into_iter()
            .for_each(|request| self.record_in_flight_cancel(request))
    }

    fn record_in_flight_opens<'a>(
        &mut self,
        requests: impl IntoIterator<Item = &'a OrderRequestOpen<ExchangeKey, InstrumentKey>>,
    ) where
        ExchangeKey: 'a,
        InstrumentKey: 'a,
    {
        requests
            .into_iter()
            .for_each(|request| self.record_in_flight_open(request))
    }

    fn record_in_flight_cancel(&mut self, request: &OrderRequestCancel<ExchangeKey, InstrumentKey>);

    fn record_in_flight_open(&mut self, request: &OrderRequestOpen<ExchangeKey, InstrumentKey>);
}

impl<ExchangeKey, InstrumentKey> InFlightRequestRecorder<ExchangeKey, InstrumentKey>
    for Orders<ExchangeKey, InstrumentKey>
where
    ExchangeKey: Debug + Clone,
    InstrumentKey: Debug + Clone,
{
    fn record_in_flight_cancel(
        &mut self,
        request: &OrderRequestCancel<ExchangeKey, InstrumentKey>,
    ) {
        let Some(order) = self.0.get_mut(&request.key.cid) else {
            error!(
                cid = %request.key.cid,
                event = ?request,
                "OrderManager cannot mark CancelInFlight for untracked Order - ignoring"
            );
            return;
        };

        order.state = ActiveOrderState::CancelInFlight(CancelInFlight {
            order: order.state.open_meta().cloned(),
        });
    }

    fn record_in_flight_open(&mut self, request: &OrderRequestOpen<ExchangeKey, InstrumentKey>) {
        if let Some(duplicate_cid_order) =
            self.0.insert(request.key.cid.clone(), Order::from(request))
        {
            error!(
                cid = %duplicate_cid_order.key.cid,
                event = ?duplicate_cid_order,
                "OrderManager upserted Order OpenInFlight with duplicate ClientOrderId"
            );
        }
    }
}

impl<GlobalData, InstrumentData> InFlightRequestRecorder<ExchangeIndex, InstrumentIndex>
    for EngineState<GlobalData, InstrumentData>
where
    InstrumentData: InFlightRequestRecorder<ExchangeIndex, InstrumentIndex>,
{
    fn record_in_flight_cancel(
        &mut self,
        request: &OrderRequestCancel<ExchangeIndex, InstrumentIndex>,
    ) {
        let instrument_state = self
            .instruments
            .instrument_index_mut(&request.key.instrument);

        instrument_state.orders.record_in_flight_cancel(request);
        instrument_state.data.record_in_flight_cancel(request);
    }

    fn record_in_flight_open(
        &mut self,
        request: &OrderRequestOpen<ExchangeIndex, InstrumentIndex>,
    ) {
        let instrument_state = self
            .instruments
            .instrument_index_mut(&request.key.instrument);

        instrument_state.orders.record_in_flight_open(request);
        instrument_state.data.record_in_flight_open(request);
    }
}
