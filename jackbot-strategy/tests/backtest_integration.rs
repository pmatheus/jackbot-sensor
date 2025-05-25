use jackbot_strategy::{CountingStrategy};
use Jackbot::{
    backtest::{backtest, BacktestArgsConstant, BacktestArgsDynamic, market_data::MarketDataInMemory},
    engine::{state::{EngineState, builder::EngineStateBuilder, global::DefaultGlobalData, instrument::data::DefaultInstrumentMarketData, trading::TradingState, instrument::{data::InstrumentDataState, filter::InstrumentFilter}}, Engine},
    risk::DefaultRiskManager,
    statistic::time::Daily,
    strategy::{
        algo::AlgoStrategy,
        close_positions::{ClosePositionsStrategy, close_open_positions_with_market_orders},
        framework,
        on_disconnect::OnDisconnectStrategy,
        on_trading_disabled::OnTradingDisabled,
    },
    system::config::ExecutionConfig,
};
use jackbot_execution::{
    client::mock::MockExecutionConfig,
    AccountSnapshot,
    order::{
        OrderKey,
        OrderKind,
        TimeInForce,
        id::{ClientOrderId, StrategyId},
        request::{OrderRequestOpen, OrderRequestCancel, RequestOpen},
    },
};
use jackbot_data::{event::{MarketEvent, DataKind}, streams::consumer::MarketStreamEvent, subscription::trade::PublicTrade};
use jackbot_instrument::{
    exchange::{ExchangeId, ExchangeIndex},
    index::IndexedInstruments,
    instrument::{Instrument, InstrumentIndex},
    asset::AssetIndex,
    Side, Underlying,
};
use rust_decimal::Decimal;
use chrono::Utc;
use smol_str::SmolStr;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::marker::PhantomData;

#[derive(Clone)]
struct CountingAdapter<State> {
    inner: Arc<Mutex<CountingStrategy>>, 
    order_sent: Arc<AtomicBool>,
    id: StrategyId,
    phantom: PhantomData<State>,
}

impl<State> Default for CountingAdapter<State> {
    fn default() -> Self {
        Self {
            inner: Arc::new(Mutex::new(CountingStrategy::default())),
            order_sent: Arc::new(AtomicBool::new(false)),
            id: StrategyId::new("counting_adapter"),
            phantom: PhantomData,
        }
    }
}

impl<GlobalData, InstrumentData> AlgoStrategy for CountingAdapter<EngineState<GlobalData, InstrumentData>>
where
    InstrumentData: InstrumentDataState,
{
    type State = EngineState<GlobalData, InstrumentData>;

    fn generate_algo_orders(&self, _state: &Self::State) -> (
        impl IntoIterator<Item = OrderRequestCancel<ExchangeIndex, InstrumentIndex>>,
        impl IntoIterator<Item = OrderRequestOpen<ExchangeIndex, InstrumentIndex>>,
    ) {
        self.inner.lock().unwrap().on_event(&());
        if self.order_sent.swap(true, Ordering::SeqCst) {
            (Vec::new(), Vec::new())
        } else {
            let open = OrderRequestOpen {
                key: OrderKey {
                    exchange: ExchangeIndex(0),
                    instrument: InstrumentIndex(0),
                    strategy: self.id.clone(),
                    cid: ClientOrderId::random(),
                },
                state: RequestOpen {
                    side: Side::Buy,
                    price: Decimal::new(100, 0),
                    quantity: Decimal::ONE,
                    kind: OrderKind::Market,
                    time_in_force: TimeInForce::ImmediateOrCancel,
                },
            };
            (Vec::<OrderRequestCancel<_, _>>::new(), vec![open])
        }
    }
}

impl<GlobalData, InstrumentData> ClosePositionsStrategy for CountingAdapter<EngineState<GlobalData, InstrumentData>>
where
    InstrumentData: InstrumentDataState,
{
    type State = EngineState<GlobalData, InstrumentData>;

    fn close_positions_requests<'a>(
        &'a self,
        state: &'a Self::State,
        filter: &'a InstrumentFilter,
    ) -> (
        impl IntoIterator<Item = OrderRequestCancel<ExchangeIndex, InstrumentIndex>> + 'a,
        impl IntoIterator<Item = OrderRequestOpen<ExchangeIndex, InstrumentIndex>> + 'a,
    )
    where
        ExchangeIndex: 'a,
        AssetIndex: 'a,
        InstrumentIndex: 'a,
    {
        close_open_positions_with_market_orders(&self.id, state, filter, |_| ClientOrderId::random())
    }
}

impl<Clock, State, ExecutionTxs, Risk> OnDisconnectStrategy<Clock, State, ExecutionTxs, Risk> for CountingAdapter<State> {
    type OnDisconnect = (); 
    fn on_disconnect(_: &mut Engine<Clock, State, ExecutionTxs, Self, Risk>, _: ExchangeId) -> Self::OnDisconnect { }
}

impl<Clock, State, ExecutionTxs, Risk> OnTradingDisabled<Clock, State, ExecutionTxs, Risk> for CountingAdapter<State> {
    type OnTradingDisabled = (); 
    fn on_trading_disabled(_: &mut Engine<Clock, State, ExecutionTxs, Self, Risk>) -> Self::OnTradingDisabled { }
}

impl<State> framework::Strategy for CountingAdapter<State> {
    type State = State;
    fn id(&self) -> StrategyId { self.id.clone() }
}

#[tokio::test]
async fn backtest_with_counting_adapter() {
    let instruments = IndexedInstruments::builder()
        .add_instrument(Instrument::spot(
            ExchangeId::BinanceSpot,
            "binance_spot_btc_usdt",
            "BTCUSDT",
            Underlying::new("btc", "usdt"),
            None,
        ))
        .build();

    let event1 = MarketStreamEvent::Item(MarketEvent {
        time_exchange: Utc::now(),
        time_received: Utc::now(),
        exchange: ExchangeId::BinanceSpot,
        instrument: InstrumentIndex(0),
        kind: DataKind::Trade(PublicTrade { id: "1".into(), price: 100.0, amount: 1.0, side: Side::Buy }),
    });
    let event2 = event1.clone();
    let market_data = MarketDataInMemory::new(Arc::new(vec![event1, event2]));
    let time_engine_start = market_data.time_first_event().await.unwrap();

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

    let strategy = CountingAdapter::<EngineState<DefaultGlobalData, DefaultInstrumentMarketData>>::default();
    let counter_ref = strategy.inner.clone();
    let args_dynamic = BacktestArgsDynamic {
        id: SmolStr::new("counting"),
        risk_free_return: Decimal::ZERO,
        strategy,
        risk: DefaultRiskManager::<EngineState<DefaultGlobalData, DefaultInstrumentMarketData>>::default(),
    };

    let summary = backtest(args_constant, args_dynamic).await.expect("backtest");
    assert_eq!(summary.id, SmolStr::new("counting"));
    assert_eq!(counter_ref.lock().unwrap().count, 2);
    assert!(summary.trading_summary.trading_duration().num_seconds() >= 0);
}

