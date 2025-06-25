pub mod advanced;
pub mod always_maker;
pub mod smart_router;
pub mod twap;
pub mod vwap;

// Wave 2 Advanced Order Types
pub mod advanced_arbitrage;
pub mod cross_exchange;
pub mod dark_pool;
pub mod iceberg;
pub mod implementation_shortfall;
pub mod pov;

// Event-driven strategy framework
pub mod event_driven;
pub mod events;
pub mod sensor_manager;
pub mod sensor_strategies;

#[cfg(test)]
pub mod integration_test;

pub use advanced::OrderExecutionStrategy;
pub use always_maker::{AlwaysMaker, AlwaysMakerConfig};
pub use smart_router::SmartRouter;
pub use twap::{TwapConfig, TwapScheduler};
pub use vwap::{VwapConfig, VwapScheduler};

// Wave 2 Advanced Order Types
pub use advanced_arbitrage::{
    AdvancedArbitrageConfig, AdvancedArbitrageEngine, ArbitrageOpportunity,
};
pub use cross_exchange::{CrossExchangeConfig, CrossExchangeRouter};
pub use dark_pool::{DarkPoolConfig, DarkPoolRouter, DarkPoolType};
pub use iceberg::{IcebergConfig, IcebergExecutor};
pub use implementation_shortfall::{
    ImplementationShortfallConfig, ImplementationShortfallExecutor,
};
pub use pov::{PovConfig, PovExecutor};

// Event-driven strategy framework
pub use events::{
    EventDrivenStrategy, EventDrivenStrategyEngine, EventFilter, MarketEvent, StrategyContext,
    StrategyError, StrategyMetrics, StrategySignal, MAX_STRATEGY_EVALUATION_TIME,
};
pub use sensor_manager::{
    SensorStrategyManager, SensorStrategyParameters, SensorStrategyRequest, SensorStrategyType,
    StrategyInfo, StrategyStatus,
};
pub use sensor_strategies::{
    SensorIcebergStrategy, SensorPovStrategy, SensorTwapStrategy, SensorVwapStrategy,
};
