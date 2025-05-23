use Jackbot::{
    backtest::{backtest, BacktestArgsConstant, BacktestArgsDynamic, market_data::MarketDataInMemory},
    engine::{state::{EngineState, builder::EngineStateBuilder, global::DefaultGlobalData, instrument::data::DefaultInstrumentMarketData, trading::TradingState}},
    risk::DefaultRiskManager,
    strategy::DefaultStrategy,
    statistic::time::Daily,
    system::config::ExecutionConfig,
};
use jackbot_data::{event::{MarketEvent, DataKind}, streams::consumer::MarketStreamEvent, subscription::trade::PublicTrade};
use jackbot_execution::{client::mock::MockExecutionConfig, AccountSnapshot};
use jackbot_instrument::{exchange::ExchangeId, index::IndexedInstruments, instrument::{Instrument, InstrumentIndex}, Side, Underlying};
use rust_decimal::Decimal;
use chrono::{Utc, Duration};
use smol_str::SmolStr;
use std::sync::Arc;

#[tokio::test]
async fn test_backtest_trading_duration() {
    // fixed times for determinism
    let start = Utc::now();
    let events = vec![
        MarketStreamEvent::Item(MarketEvent {
            time_exchange: start,
            time_received: start,
            exchange: ExchangeId::BinanceSpot,
            instrument: InstrumentIndex(0),
            kind: DataKind::Trade(PublicTrade { id: "1".into(), price: 100.0, amount: 1.0, side: Side::Buy }),
        }),
        MarketStreamEvent::Item(MarketEvent {
            time_exchange: start + Duration::seconds(1),
            time_received: start + Duration::seconds(1),
            exchange: ExchangeId::BinanceSpot,
            instrument: InstrumentIndex(0),
            kind: DataKind::Trade(PublicTrade { id: "2".into(), price: 101.0, amount: 1.0, side: Side::Buy }),
        }),
    ];
    let market_data = MarketDataInMemory::new(Arc::new(events));
    let time_engine_start = market_data.time_first_event().await.unwrap();

    let instruments = IndexedInstruments::builder()
        .add_instrument(Instrument::spot(
            ExchangeId::BinanceSpot,
            "binance_spot_btc_usdt",
            "BTCUSDT",
            Underlying::new("btc", "usdt"),
            None,
        ))
        .build();

    let engine_state: EngineState<DefaultGlobalData, DefaultInstrumentMarketData> =
        EngineStateBuilder::new(&instruments, DefaultGlobalData::default(), DefaultInstrumentMarketData::default)
            .time_engine_start(time_engine_start)
            .trading_state(TradingState::Enabled)
            .build();

    let execution_snapshot = AccountSnapshot { exchange: ExchangeId::BinanceSpot, balances: Vec::new(), instruments: Vec::new() };
    let executions = vec![ExecutionConfig::Mock(MockExecutionConfig { mocked_exchange: ExchangeId::BinanceSpot, initial_state: execution_snapshot, latency_ms: 0, fees_percent: Decimal::ZERO })];

    let args_constant = Arc::new(BacktestArgsConstant {
        instruments,
        executions,
        market_data,
        summary_interval: Daily,
        engine_state,
    });

    let args_dynamic = BacktestArgsDynamic {
        id: SmolStr::new("duration"),
        risk_free_return: Decimal::ZERO,
        strategy: DefaultStrategy::<EngineState<DefaultGlobalData, DefaultInstrumentMarketData>>::default(),
        risk: DefaultRiskManager::<EngineState<DefaultGlobalData, DefaultInstrumentMarketData>>::default(),
    };

    let summary = backtest(args_constant, args_dynamic).await.expect("backtest");
    assert_eq!(summary.id, SmolStr::new("duration"));
    assert_eq!(summary.trading_summary.trading_duration().num_seconds(), 1);
}
