use crate::strategy::{algo::AlgoStrategy, close_positions::ClosePositionsStrategy};
use jackbot_execution::order::id::StrategyId;
use jackbot_instrument::{asset::AssetIndex, exchange::ExchangeIndex, instrument::InstrumentIndex};

pub trait Strategy<StateTy, E = ExchangeIndex, I = InstrumentIndex>:
    AlgoStrategy<E, I, State = StateTy> + ClosePositionsStrategy<E, AssetIndex, I, State = StateTy>
{
    fn id(&self) -> StrategyId;
}
