use jackbot_execution::AccountSnapshot;
/// Configuration module for trading system components.
///
/// Provides data structures for configuring various aspects of a trading system,
/// including instruments and execution components.
// pub use jackbot_execution::client::mock::MockExecutionConfig;
use jackbot_execution::client::mock::MockExecutionConfig as ExternalMockExecutionConfig;
use jackbot_instrument::{
    Underlying,
    asset::{Asset, name::AssetNameExchange},
    exchange::ExchangeId,
    instrument::{
        Instrument,
        kind::{
            InstrumentKind, future::FutureContract, option::OptionContract,
            perpetual::PerpetualContract,
        },
        name::{InstrumentNameExchange, InstrumentNameInternal},
        quote::InstrumentQuoteAsset,
        spec::{InstrumentSpec, InstrumentSpecQuantity, OrderQuantityUnits},
    },
};
use rust_decimal::Decimal;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::cmp::Ordering;
use std::hash::{Hash, Hasher};

/// Top-level configuration for a full trading system.
///
/// Contains configuration for all instruments and execution components.
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Deserialize, Serialize)]
pub struct SystemConfig {
    /// Configurations for all instruments the system will track.
    pub instruments: Vec<InstrumentConfig>,

    /// Configurations for all execution components.
    pub executions: Vec<ExecutionConfig>,
}

/// Convenient minimal instrument configuration, used to generate an [`Instrument`] on startup.
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Deserialize, Serialize)]
pub struct InstrumentConfig {
    /// Exchange identifier where the instrument is traded.
    pub exchange: ExchangeId,

    /// Exchange-specific name for the instrument (e.g., "BTCUSDT").
    pub name_exchange: InstrumentNameExchange,

    /// Underlying asset pair for the instrument.
    pub underlying: Underlying<AssetNameExchange>,

    /// Quote asset for the instrument.
    pub quote: InstrumentQuoteAsset,

    /// Type of the instrument (spot, perpetual, future, option).
    pub kind: InstrumentKind<AssetNameExchange>,

    /// Optional additional specifications for the instrument.
    pub spec: Option<InstrumentSpec<AssetNameExchange>>,
}

// Shadow struct for serialization/deserialization
#[derive(Serialize, Deserialize)]
struct MockExecutionConfigShadow {
    mocked_exchange: ExchangeId,
    initial_state: AccountSnapshot,
    latency_ms: u64,
    fees_percent: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalMockExecutionConfig(pub ExternalMockExecutionConfig);

impl Serialize for LocalMockExecutionConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let shadow = MockExecutionConfigShadow {
            mocked_exchange: self.0.mocked_exchange,
            initial_state: self.0.initial_state.clone(),
            latency_ms: self.0.latency_ms,
            fees_percent: self.0.fees_percent,
        };
        shadow.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for LocalMockExecutionConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let shadow = MockExecutionConfigShadow::deserialize(deserializer)?;
        Ok(LocalMockExecutionConfig(ExternalMockExecutionConfig {
            mocked_exchange: shadow.mocked_exchange,
            initial_state: shadow.initial_state,
            latency_ms: shadow.latency_ms,
            fees_percent: shadow.fees_percent,
        }))
    }
}

impl PartialOrd for LocalMockExecutionConfig {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        // Order based on mocked_exchange, then latency_ms, then fees_percent.
        // initial_state is ignored for ordering due to its complexity.
        match self.0.mocked_exchange.partial_cmp(&other.0.mocked_exchange) {
            Some(Ordering::Equal) | None => {}
            Some(other) => return Some(other),
        }
        match self.0.latency_ms.partial_cmp(&other.0.latency_ms) {
            Some(Ordering::Equal) | None => {}
            Some(other) => return Some(other),
        }
        self.0.fees_percent.partial_cmp(&other.0.fees_percent)
    }
}

impl Ord for LocalMockExecutionConfig {
    fn cmp(&self, other: &Self) -> Ordering {
        // Order based on mocked_exchange, then latency_ms, then fees_percent.
        self.0
            .mocked_exchange
            .cmp(&other.0.mocked_exchange)
            .then_with(|| self.0.latency_ms.cmp(&other.0.latency_ms))
            .then_with(|| self.0.fees_percent.cmp(&other.0.fees_percent))
    }
}

impl Hash for LocalMockExecutionConfig {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Hash based on mocked_exchange, latency_ms, and fees_percent.
        // initial_state is ignored for hashing.
        self.0.mocked_exchange.hash(state);
        self.0.latency_ms.hash(state);
        self.0.fees_percent.hash(state); // Decimal should be hashable
    }
}

/// Configuration for an execution link.
///
/// Represents different types of execution configurations.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize, Serialize)]
pub enum ExecutionConfig {
    /// Placeholder variant (mock execution config not available)
    None,
    Mock(LocalMockExecutionConfig),
}

impl From<InstrumentConfig> for Instrument<ExchangeId, Asset> {
    fn from(value: InstrumentConfig) -> Self {
        Self {
            exchange: value.exchange,
            name_internal: InstrumentNameInternal::new_from_exchange_underlying(
                value.exchange,
                &value.underlying.base,
                &value.underlying.quote,
            ),
            name_exchange: value.name_exchange,
            underlying: Underlying {
                base: Asset::new_from_exchange(value.underlying.base),
                quote: Asset::new_from_exchange(value.underlying.quote),
            },
            quote: value.quote,
            kind: match value.kind {
                InstrumentKind::Spot => InstrumentKind::Spot,
                InstrumentKind::Perpetual(contract) => {
                    InstrumentKind::Perpetual(PerpetualContract {
                        contract_size: contract.contract_size,
                        settlement_asset: Asset::new_from_exchange(contract.settlement_asset),
                    })
                }
                InstrumentKind::Future(contract) => InstrumentKind::Future(FutureContract {
                    contract_size: contract.contract_size,
                    settlement_asset: Asset::new_from_exchange(contract.settlement_asset),
                    expiry: contract.expiry,
                }),
                InstrumentKind::Option(contract) => InstrumentKind::Option(OptionContract {
                    contract_size: contract.contract_size,
                    settlement_asset: Asset::new_from_exchange(contract.settlement_asset),
                    kind: contract.kind,
                    exercise: contract.exercise,
                    expiry: contract.expiry,
                    strike: contract.strike,
                }),
            },
            spec: value.spec.map(|spec| InstrumentSpec {
                price: spec.price,
                quantity: InstrumentSpecQuantity {
                    unit: match spec.quantity.unit {
                        OrderQuantityUnits::Asset(asset) => {
                            OrderQuantityUnits::Asset(Asset::new_from_exchange(asset))
                        }
                        OrderQuantityUnits::Contract => OrderQuantityUnits::Contract,
                        OrderQuantityUnits::Quote => OrderQuantityUnits::Quote,
                    },
                    min: spec.quantity.min,
                    increment: spec.quantity.increment,
                },
                notional: spec.notional,
            }),
        }
    }
}
