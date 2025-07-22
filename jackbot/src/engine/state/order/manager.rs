use crate::engine::state::order::{in_flight_recorder::InFlightRequestRecorder, orders::Orders};
use jackbot_execution::order::{
    Order,
    request::OrderResponseCancel,
    state::{ActiveOrderState, CancelInFlight, OrderState},
};
use jackbot_integration::snapshot::Snapshot;
use std::{collections::hash_map::Entry, fmt::Debug};
use tracing::{debug, error, warn};

/// Synchronous order manager that tracks the lifecycle of active exchange orders.
///
/// See [`Orders`](super::Orders) for an example implementation.
pub trait OrderManager<ExchangeKey, InstrumentKey>
where
    Self: InFlightRequestRecorder<ExchangeKey, InstrumentKey>,
{
    fn orders<'a>(
        &'a self,
    ) -> impl Iterator<Item = &'a Order<ExchangeKey, InstrumentKey, ActiveOrderState>>
    where
        ExchangeKey: 'a,
        InstrumentKey: 'a;

    fn update_from_order_snapshot<AssetKey>(
        &mut self,
        snapshot: Snapshot<&Order<ExchangeKey, InstrumentKey, OrderState<AssetKey, InstrumentKey>>>,
    ) where
        AssetKey: Debug + Clone;

    fn update_from_cancel_response<AssetKey>(
        &mut self,
        response: &OrderResponseCancel<ExchangeKey, AssetKey, InstrumentKey>,
    ) where
        AssetKey: Debug + Clone;
}

impl<ExchangeKey, InstrumentKey> OrderManager<ExchangeKey, InstrumentKey>
    for Orders<ExchangeKey, InstrumentKey>
where
    ExchangeKey: Debug + Clone,
    InstrumentKey: Debug + Clone,
{
    fn orders<'a>(
        &'a self,
    ) -> impl Iterator<Item = &'a Order<ExchangeKey, InstrumentKey, ActiveOrderState>>
    where
        ExchangeKey: 'a,
        InstrumentKey: 'a,
    {
        self.0.values()
    }

    fn update_from_order_snapshot<AssetKey>(
        &mut self,
        snapshot: Snapshot<&Order<ExchangeKey, InstrumentKey, OrderState<AssetKey, InstrumentKey>>>,
    ) where
        AssetKey: Debug + Clone,
    {
        let Snapshot(snapshot) = snapshot;

        let (mut current_entry, update) = match (
            self.0.entry(snapshot.key.cid.clone()),
            snapshot.to_active(),
        ) {
            // Order untracked, input Snapshot is InactiveOrderState (ie/ finished), so ignore
            (Entry::Vacant(_), None) => {
                warn!(
                    exchange = ?snapshot.key.exchange,
                    instrument = ?snapshot.key.instrument,
                    strategy = %snapshot.key.strategy,
                    cid = %snapshot.key.cid,
                    update = ?snapshot,
                    "OrderManager received inactive order snapshot for untracked order - ignoring"
                );
                return;
            }

            // Order untracked, input Snapshot is ActiveOrderState, so insert
            (Entry::Vacant(entry), Some(update)) => {
                match &update.state {
                    ActiveOrderState::Open(open)
                        if open.quantity_remaining(update.quantity).is_zero() =>
                    {
                        debug!(
                            exchange = ?snapshot.key.exchange,
                            instrument = ?snapshot.key.instrument,
                            strategy = %snapshot.key.strategy,
                            cid = %snapshot.key.cid,
                            update = ?snapshot,
                            "OrderManager ignoring new Open order which is actually FulledFilled"
                        );
                    }
                    _active_order => {
                        debug!(
                            exchange = ?snapshot.key.exchange,
                            instrument = ?snapshot.key.instrument,
                            strategy = %snapshot.key.strategy,
                            cid = %snapshot.key.cid,
                            update = ?snapshot,
                            "OrderManager tracking new order"
                        );
                        entry.insert(update);
                    }
                }
                return;
            }

            // Order tracked, input Snapshot is InactiveOrderState (ie/ finished), so remove
            (Entry::Occupied(entry), None) => {
                debug!(
                    exchange = ?snapshot.key.exchange,
                    instrument = ?snapshot.key.instrument,
                    strategy = %snapshot.key.strategy,
                    cid = %snapshot.key.cid,
                    update = ?snapshot,
                    "OrderManager received inactive order snapshot for tracked order - removing"
                );
                entry.remove();
                return;
            }

            // Order tracked, input Snapshot is ActiveOrderState, so forward for further processing
            (Entry::Occupied(entry), Some(update)) => (entry, update),
        };

        match (&current_entry.get().state, update.state) {
            (ActiveOrderState::OpenInFlight(_), ActiveOrderState::OpenInFlight(_)) => {
                warn!(
                    exchange = ?snapshot.key.exchange,
                    instrument = ?snapshot.key.instrument,
                    strategy = %snapshot.key.strategy,
                    cid = %snapshot.key.cid,
                    update = ?snapshot,
                    "OrderManager received a duplicate OpenInFlight recording - ignoring"
                );
            }
            (ActiveOrderState::OpenInFlight(_), ActiveOrderState::Open(open)) => {
                debug!(
                    exchange = ?snapshot.key.exchange,
                    instrument = ?snapshot.key.instrument,
                    strategy = %snapshot.key.strategy,
                    cid = %snapshot.key.cid,
                    update = ?snapshot,
                    "OrderManager transitioned an OpenInFlight order to Open"
                );
                if open.quantity_remaining(update.quantity).is_zero() {
                    current_entry.remove();
                } else {
                    current_entry.get_mut().state = ActiveOrderState::Open(open);
                }
            }
            (ActiveOrderState::OpenInFlight(_), ActiveOrderState::CancelInFlight(update)) => {
                debug!(
                    exchange = ?snapshot.key.exchange,
                    instrument = ?snapshot.key.instrument,
                    strategy = %snapshot.key.strategy,
                    cid = %snapshot.key.cid,
                    update = ?snapshot,
                    "OrderManager transitioned an OpenInFlight order to CancelInFlight"
                );
                current_entry.get_mut().state = ActiveOrderState::CancelInFlight(update);
            }
            (ActiveOrderState::Open(_), ActiveOrderState::OpenInFlight(_)) => {
                warn!(
                    exchange = ?snapshot.key.exchange,
                    instrument = ?snapshot.key.instrument,
                    strategy = %snapshot.key.strategy,
                    cid = %snapshot.key.cid,
                    update = ?snapshot,
                    "OrderManager received an OpenInFlight recording for an Open order - ignoring"
                );
            }
            (ActiveOrderState::Open(current), ActiveOrderState::Open(update)) => {
                if current.time_exchange <= update.time_exchange {
                    debug!(
                        exchange = ?snapshot.key.exchange,
                        instrument = ?snapshot.key.instrument,
                        strategy = %snapshot.key.strategy,
                        cid = %snapshot.key.cid,
                        update = ?snapshot,
                        "OrderManager updating an Open order from a more recent snapshot"
                    );
                    current_entry.get_mut().state = ActiveOrderState::Open(update);
                } else {
                    debug!(
                        exchange = ?snapshot.key.exchange,
                        instrument = ?snapshot.key.instrument,
                        strategy = %snapshot.key.strategy,
                        cid = %snapshot.key.cid,
                        update = ?snapshot,
                        "OrderManager received an out of sequence Open order snapshot - ignoring"
                    );
                }
            }
            (ActiveOrderState::Open(current), ActiveOrderState::CancelInFlight(mut update)) => {
                debug!(
                    exchange = ?snapshot.key.exchange,
                    instrument = ?snapshot.key.instrument,
                    strategy = %snapshot.key.strategy,
                    cid = %snapshot.key.cid,
                    update = ?snapshot,
                    "OrderManager transitioned an Open order to CancelInFlight"
                );

                // Ensure next CancelInFlight.Open is populated and the most recent
                let latest_open = update
                    .order
                    .take()
                    .filter(|update| current.time_exchange <= update.time_exchange)
                    .unwrap_or_else(|| current.clone());

                current_entry.get_mut().state = ActiveOrderState::CancelInFlight(CancelInFlight {
                    order: Some(latest_open),
                })
            }
            (ActiveOrderState::CancelInFlight(_), ActiveOrderState::OpenInFlight(_)) => {
                error!(
                    exchange = ?snapshot.key.exchange,
                    instrument = ?snapshot.key.instrument,
                    strategy = %snapshot.key.strategy,
                    cid = %snapshot.key.cid,
                    update = ?snapshot,
                    "OrderManager received an OpenInFlight recording for a CancelInFlight - ignoring"
                );
            }
            (ActiveOrderState::CancelInFlight(current), ActiveOrderState::Open(update)) => {
                debug!(
                    exchange = ?snapshot.key.exchange,
                    instrument = ?snapshot.key.instrument,
                    strategy = %snapshot.key.strategy,
                    cid = %snapshot.key.cid,
                    update = ?snapshot,
                    "OrderManager received an Open order snapshot for a CancelInFlight - updating CancelInFlight.Open"
                );

                // Check if the update Open is more recent
                let update_open_is_latest = current
                    .order
                    .as_ref()
                    .is_none_or(|current| current.time_exchange <= update.time_exchange);

                if update_open_is_latest {
                    current_entry.get_mut().state =
                        ActiveOrderState::CancelInFlight(CancelInFlight {
                            order: Some(update),
                        });
                }
            }
            (ActiveOrderState::CancelInFlight(_), ActiveOrderState::CancelInFlight(_)) => {
                warn!(
                    exchange = ?snapshot.key.exchange,
                    instrument = ?snapshot.key.instrument,
                    strategy = %snapshot.key.strategy,
                    cid = %snapshot.key.cid,
                    update = ?snapshot,
                    "OrderManager received a duplicate CancelInFlight recording - ignoring"
                );
            }
        }
    }

    fn update_from_cancel_response<AssetKey>(
        &mut self,
        response: &OrderResponseCancel<ExchangeKey, AssetKey, InstrumentKey>,
    ) where
        AssetKey: Debug + Clone,
    {
        let Entry::Occupied(mut order) = self.0.entry(response.key.cid.clone()) else {
            warn!(
                exchange = ?response.key.exchange,
                instrument = ?response.key.instrument,
                strategy = %response.key.strategy,
                cid = %response.key.cid,
                update = ?response,
                "OrderManager received an OrderResponseCancel for untracked order - ignoring"
            );
            return;
        };

        match (&order.get().state, &response.state) {
            (ActiveOrderState::OpenInFlight(_) | ActiveOrderState::Open(_), Ok(_)) => {
                warn!(
                    exchange = ?response.key.exchange,
                    instrument = ?response.key.instrument,
                    strategy = %response.key.strategy,
                    cid = %response.key.cid,
                    update = ?response,
                    "OrderManager received Ok(Cancelled) for tracked order not CancelInFlight - removing"
                );
                order.remove();
            }
            (ActiveOrderState::CancelInFlight(_), Ok(_)) => {
                debug!(
                    exchange = ?response.key.exchange,
                    instrument = ?response.key.instrument,
                    strategy = %response.key.strategy,
                    cid = %response.key.cid,
                    update = ?response,
                    "OrderManager received Ok(Cancelled) for tracked order CancelInFlight - removing"
                );
                order.remove();
            }
            (ActiveOrderState::OpenInFlight(_) | ActiveOrderState::Open(_), Err(error)) => {
                warn!(
                    exchange = ?response.key.exchange,
                    instrument = ?response.key.instrument,
                    strategy = %response.key.strategy,
                    cid = %response.key.cid,
                    update = ?response,
                    ?error,
                    "OrderManager received Err(Cancelled) for tracked order not CancelInFlight - ignoring"
                );
            }
            (ActiveOrderState::CancelInFlight(in_flight_cancel), Err(error)) => {
                // Expected, keep move to Open
                if let Some(open) = &in_flight_cancel.order {
                    debug!(
                        exchange = ?response.key.exchange,
                        instrument = ?response.key.instrument,
                        strategy = %response.key.strategy,
                        cid = %response.key.cid,
                        update = ?response,
                        ?error,
                        "OrderManager received Err(Cancelled) for previously Open order - setting Open"
                    );
                    order.get_mut().state = ActiveOrderState::Open(open.clone())
                } else {
                    debug!(
                        exchange = ?response.key.exchange,
                        instrument = ?response.key.instrument,
                        strategy = %response.key.strategy,
                        cid = %response.key.cid,
                        update = ?response,
                        ?error,
                        "OrderManager received Err(Cancelled) for previously non-Open order - removing"
                    );
                    // Likely previously OpenInFlight, and attempted cancel before Open snapshot
                    // -> it's expected that an Order snapshot is inbound
                    order.remove();
                }
            }
        }
    }
}
