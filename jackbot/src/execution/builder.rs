use crate::{
    engine::execution_tx::MultiExchangeTxMap,
    error::JackbotError,
    execution::{
        AccountStreamEvent, error::ExecutionError, manager::ExecutionManager,
        request::ExecutionRequest,
    },
};
use std::future::Future;
use fnv::FnvHashMap;
use futures::{FutureExt, future::join_all};
use jackbot_data::streams::consumer::STREAM_RECONNECTION_POLICY;
use jackbot_data::streams::reconnect::stream::ReconnectingStream;
use jackbot_execution::{
    UnindexedAccountSnapshot,
    client::{
        ExecutionClient,
        binance::paper::{BinancePaperClient, BinancePaperConfig},
    },
    indexer::AccountEventIndexer,
    map::generate_execution_instrument_map,
};
use jackbot_instrument::{
    asset::AssetIndex,
    exchange::{ExchangeId, ExchangeIndex},
    index::IndexedInstruments,
    instrument::InstrumentIndex,
};
use jackbot_integration::channel::{Channel, UnboundedTx, mpsc_unbounded};
use std::{pin::Pin, sync::Arc, time::Duration};
use tokio::task::{JoinError, JoinHandle};

type ExecutionInitFuture =
    Pin<Box<dyn Future<Output = Result<(RunFuture, RunFuture), ExecutionError>> + Send>>;
type RunFuture = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

/// Collection of execution initialization futures.
pub type ExecutionBuildFutures = Vec<ExecutionInitFuture>;

/// Collection of execution component join handles.
pub struct ExecutionHandles {
    pub handles: Vec<JoinHandle<()>>,
}

impl std::fmt::Debug for ExecutionHandles {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExecutionHandles")
            .field("handles", &format!("{} join handles", self.handles.len()))
            .finish()
    }
}

impl IntoIterator for ExecutionHandles {
    type Item = JoinHandle<()>;
    type IntoIter = std::vec::IntoIter<JoinHandle<()>>;

    fn into_iter(self) -> Self::IntoIter {
        self.handles.into_iter()
    }
}

impl ExecutionHandles {
    /// Shutdown all execution components concurrently.
    pub async fn shutdown(&mut self) -> Result<(), JoinError> {
        let mut handles = Vec::new();
        std::mem::swap(&mut handles, &mut self.handles);

        join_all(handles)
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .map(|_| ())
    }
}

/// Full execution infrastructure builder.
///
/// Add Mock and Live [`ExecutionClient`] configurations and let the builder set up the required
/// infrastructure.
///
/// Once you have added all the configurations, call [`ExecutionBuilder::build`] to return the
/// full [`ExecutionBuild`]. Then calling [`ExecutionBuild::init`] will then initialise
/// the built infrastructure.
///
/// Handles:
/// - Building mock execution managers (mocks a specific exchange internally via the [`MockExchange`]).
/// - Building live execution managers, setting up an external connection to each exchange.
/// - Constructs a [`MultiExchangeTxMap`] with an entry for each mock/live execution manager.
/// - Combines all exchange account streams into a unified [`AccountStreamEvent`] `Stream`.
#[allow(missing_debug_implementations)]
pub struct ExecutionBuilder<'a> {
    instruments: &'a IndexedInstruments,
    execution_txs: FnvHashMap<ExchangeId, (ExchangeIndex, UnboundedTx<ExecutionRequest>)>,
    merged_channel: Channel<AccountStreamEvent<ExchangeIndex, AssetIndex, InstrumentIndex>>,
    execution_init_futures: Vec<ExecutionInitFuture>,
}

impl<'a> ExecutionBuilder<'a> {
    /// Construct a new `ExecutionBuilder` using the provided `IndexedInstruments`.
    pub fn new(instruments: &'a IndexedInstruments) -> Self {
        Self {
            instruments,
            execution_txs: FnvHashMap::default(),
            merged_channel: Channel::default(),
            execution_init_futures: Vec::default(),
        }
    }

    /// Adds an [`ExecutionManager`] for a mock exchange using `BinancePaperClient`.
    pub fn add_mock(
        self,
        config: crate::system::config::LocalMockExecutionConfig,
        request_timeout: Duration,
    ) -> Result<Self, JackbotError> {
        // Build mock execution with provided mock configuration
        let fees_percent = config.0.fees_percent;
        // Use an empty snapshot; initial_state is not used in this simplified mock
        let snapshot = UnindexedAccountSnapshot {
            exchange: config.0.mocked_exchange,
            balances: Vec::new(),
            instruments: Vec::new(),
        };
        let paper_config = BinancePaperConfig {
            books: FnvHashMap::default(),
            instruments: FnvHashMap::default(),
            snapshot,
            fees_percent,
        };

        // Use provided latency_ms as request timeout or fallback to provided timeout
        // (The request_timeout parameter is still honored)
        if config.0.mocked_exchange != ExchangeId::BinanceSpot
            && config.0.mocked_exchange != ExchangeId::BinanceFuturesUsd
        {
            // Log a warning or handle as appropriate if the configured exchange_id
            // doesn't perfectly match the hardcoded exchange in BinancePaperClient.
            // For now, we proceed, assuming the routing logic handles this.
            tracing::warn!(
                exchange_id = ?config.0.mocked_exchange,
                paper_client_exchange_id = ?BinancePaperClient::EXCHANGE,
                "Using BinancePaperClient for an exchange_id that is not explicitly BinanceSpot/BinanceFuturesUsd. Ensure routing is correct."
            );
        }

        self.add_execution::<BinancePaperClient>(
            config.0.mocked_exchange,
            paper_config,
            request_timeout,
        )
    }

    /// Adds an [`ExecutionManager`] for a live exchange.
    pub fn add_live<Client>(
        self,
        config: Client::Config,
        request_timeout: Duration,
    ) -> Result<Self, JackbotError>
    where
        Client: ExecutionClient + Send + Sync + 'static,
        Client::AccountStream: Send,
        Client::Config: Send,
    {
        self.add_execution::<Client>(Client::EXCHANGE, config, request_timeout)
    }

    fn add_execution<Client>(
        mut self,
        exchange: ExchangeId,
        config: Client::Config,
        request_timeout: Duration,
    ) -> Result<Self, JackbotError>
    where
        Client: ExecutionClient + Send + Sync + 'static,
        Client::AccountStream: Send,
        Client::Config: Send,
    {
        let instrument_map = generate_execution_instrument_map(self.instruments, exchange)?;

        let (execution_tx, execution_rx) = mpsc_unbounded();

        if self
            .execution_txs
            .insert(exchange, (instrument_map.exchange.key, execution_tx))
            .is_some()
        {
            return Err(JackbotError::ExecutionBuilder(format!(
                "ExecutionBuilder does not support duplicate mocked ExecutionManagers: {exchange}"
            )));
        }

        let merged_tx = self.merged_channel.tx.clone();

        let future_result = ExecutionManager::init(
            execution_rx.into_stream(),
            request_timeout,
            Arc::new(Client::new(config)),
            AccountEventIndexer::new(Arc::new(instrument_map)),
            STREAM_RECONNECTION_POLICY,
        );

        let future_result = future_result.map(|result| {
            result.map(|(manager, account_stream)| {
                let manager_future: RunFuture = Box::pin(manager.run());
                let stream_future: RunFuture = Box::pin(account_stream.forward_to(merged_tx));

                (manager_future, stream_future)
            })
        });

        self.execution_init_futures.push(Box::pin(future_result));

        Ok(self)
    }

    /// Consume this `ExecutionBuilder` and build a full [`ExecutionBuild`]
    pub fn build(self) -> ExecutionBuild {
        ExecutionBuild {
            execution_txs: self.execution_txs,
            merged_channel: self.merged_channel,
            execution_init_futures: self.execution_init_futures,
        }
    }
}

/// Initialised execution infrastructure build.
///
/// Constructed by calling [`ExecutionBuilder::build`]. Contains the execution instrument map,
/// [`ExecutionRequest`] and [`AccountStreamEvent`] channels, and futures to initialise the
/// execution infrastructure.
#[allow(missing_debug_implementations)]
pub struct ExecutionBuild {
    pub execution_txs: FnvHashMap<ExchangeId, (ExchangeIndex, UnboundedTx<ExecutionRequest>)>,
    pub merged_channel: Channel<AccountStreamEvent>,
    pub execution_init_futures: ExecutionBuildFutures,
}

impl ExecutionBuild {
    /// Initializes the constructed execution infrastructure.
    ///
    /// This awaits the execution initialization futures and returns the handles for
    /// the running execution components along with the [`MultiExchangeTxMap`].
    pub async fn init(
        self,
    ) -> Result<
        (
            MultiExchangeTxMap,
            Channel<AccountStreamEvent>,
            ExecutionHandles,
        ),
        ExecutionError,
    > {
        self.init_with_runtime(tokio::runtime::Handle::current())
            .await
    }

    /// Initializes the constructed execution infrastructure with a specific runtime.
    ///
    /// This awaits the execution initialization futures and returns the handles for
    /// the running execution components along with the [`MultiExchangeTxMap`].
    pub async fn init_with_runtime(
        self,
        runtime: tokio::runtime::Handle,
    ) -> Result<
        (
            MultiExchangeTxMap,
            Channel<AccountStreamEvent>,
            ExecutionHandles,
        ),
        ExecutionError,
    > {
        let Self {
            execution_txs,
            merged_channel,
            execution_init_futures,
        } = self;

        // Create MultiExchangeTxMap using its FromIterator implementation
        let execution_tx_map = execution_txs
            .into_iter()
            .map(|(exchange_id, (_exchange_index, tx))| (exchange_id, Some(tx)))
            .collect::<MultiExchangeTxMap>();

        // Initialize all execution components concurrently
        let init_results = futures::future::join_all(execution_init_futures).await;

        // Collect execution futures and run them
        let mut handles = Vec::new();
        for result in init_results {
            let (manager_future, stream_future) = result?;
            handles.push(runtime.spawn(manager_future));
            handles.push(runtime.spawn(stream_future));
        }

        Ok((
            execution_tx_map,
            merged_channel,
            ExecutionHandles { handles },
        ))
    }
}
