use crate::{
    engine::{
        Engine,
        state::{
            EngineState,
            instrument::{data::InstrumentDataState, filter::InstrumentFilter},
        },
    },
    strategy::{
        algo::AlgoStrategy,
        close_positions::{ClosePositionsStrategy, close_open_positions_with_market_orders},
        on_disconnect::OnDisconnectStrategy,
        on_trading_disabled::OnTradingDisabled,
    },
};
use jackbot_execution::order::{
    id::{ClientOrderId, StrategyId},
    request::{OrderRequestCancel, OrderRequestOpen},
};
use jackbot_instrument::{
    asset::AssetIndex,
    exchange::{ExchangeId, ExchangeIndex},
    instrument::InstrumentIndex,
};
use std::marker::PhantomData;

/// Defines a strategy interface for generating algorithmic open and cancel order requests based
/// on the current `EngineState`.
pub mod algo;

/// Defines a strategy interface for generating open and cancel order requests that close open
/// positions.
pub mod close_positions;

/// Defines a strategy interface enables custom [`Engine`] to be performed in the event of an
/// exchange disconnection.
pub mod on_disconnect;

/// Defines a strategy interface enables custom [`Engine`] to be performed in the event that the
/// `TradingState` gets set to `TradingState::Disabled`.
pub mod on_trading_disabled;

/// Advanced execution algorithms such as TWAP/VWAP slicing and always maker.
pub mod advanced_orders;

/// Strategy trait combining the core strategy interfaces.
pub mod framework;

/// Default strategy that generates no orders and does nothing on events.
#[derive(Debug, Clone)]
pub struct DefaultStrategy<StateTy, E = ExchangeIndex, I = InstrumentIndex> {
    phantom: PhantomData<(StateTy, E, I)>,
    id: StrategyId,
}

impl<StateTy> Default for DefaultStrategy<StateTy> {
    fn default() -> Self {
        DefaultStrategy {
            phantom: PhantomData,
            id: StrategyId::new("default"),
        }
    }
}

impl<StateTy, E, I> crate::strategy::algo::AlgoStrategy<E, I> for DefaultStrategy<StateTy, E, I> {
    type State = StateTy;

    fn generate_algo_orders(
        &self,
        _state: &Self::State,
    ) -> (
        impl IntoIterator<Item = OrderRequestCancel<E, I>>,
        impl IntoIterator<Item = OrderRequestOpen<E, I>>,
    ) {
        (
            Vec::<OrderRequestCancel<E, I>>::new(),
            Vec::<OrderRequestOpen<E, I>>::new(),
        )
    }
}

impl<StateTy, E, I> crate::strategy::close_positions::ClosePositionsStrategy<E, AssetIndex, I>
    for DefaultStrategy<StateTy, E, I>
{
    type State = StateTy;

    fn close_positions_requests<'a>(
        &'a self,
        _state: &'a Self::State,
        _filter: &'a InstrumentFilter<E, AssetIndex, I>,
    ) -> (
        impl IntoIterator<Item = OrderRequestCancel<E, I>> + 'a,
        impl IntoIterator<Item = OrderRequestOpen<E, I>> + 'a,
    )
    where
        E: 'a,
        AssetIndex: 'a,
        I: 'a,
    {
        (
            Vec::<OrderRequestCancel<E, I>>::new(),
            Vec::<OrderRequestOpen<E, I>>::new(),
        )
    }
}

impl<Clock, StateTy, E, I, ExecutionTxs, Risk>
    OnDisconnectStrategy<Clock, StateTy, ExecutionTxs, Risk> for DefaultStrategy<StateTy, E, I>
{
    type OnDisconnect = ();

    fn on_disconnect(
        _engine: &mut Engine<Clock, StateTy, ExecutionTxs, Self, Risk>,
        _exchange: ExchangeId,
    ) -> Self::OnDisconnect {
    }
}

impl<Clock, StateTy, E, I, ExecutionTxs, Risk> OnTradingDisabled<Clock, StateTy, ExecutionTxs, Risk>
    for DefaultStrategy<StateTy, E, I>
{
    type OnTradingDisabled = ();

    fn on_trading_disabled(
        _engine: &mut Engine<Clock, StateTy, ExecutionTxs, Self, Risk>,
    ) -> Self::OnTradingDisabled {
    }
}

impl<StateTy, E, I> framework::Strategy<StateTy, E, I> for DefaultStrategy<StateTy, E, I> {
    fn id(&self) -> StrategyId {
        self.id.clone()
    }
}
