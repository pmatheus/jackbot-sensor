use crate::{
    error::DataError,
    event::DataKind,
    instrument::MarketInstrumentData,
    streams::{
        builder::dynamic::DynamicStreams,
        consumer::{MarketStreamEvent, MarketStreamResult},
        reconnect::stream::ReconnectingStream,
    },
    subscription::{SubKind, Subscription},
};
use futures::Stream;
use itertools::Itertools;
use jackbot_instrument::{
    exchange::ExchangeId,
    index::{IndexedInstruments, Keyed, error::IndexError},
    instrument::{InstrumentIndex, market_data::MarketDataInstrument},
};
use tracing::warn;

/// Initialise an indexed [`DynamicStreams`] using batches of indexed [`Subscription`] batches.
///
/// This function:
/// 1. Generates indexed market data Subscriptions from all Instrument-SubKind combinations found
///    in the provided `IndexedInstruments` and `SubKind` slice.
/// 2. Initialise an indexed [`DynamicStreams`] .
/// 3. Combines all market streams into a single `Stream` via
///    [`select_all`](futures_util::stream::select_all::select_all)
/// 4. Handles recoverable errors by logging them at `warn` level.
///
/// See [`generate_indexed_market_data_subscription_batches`] for how indexed `Subscriptions` can
/// be conveniently generated from an [`IndexedInstruments`] collection.
///
/// See [`index_market_data_subscription_batches`] for how unindexed `Subscriptions` can be
/// indexed using an [`IndexedInstruments`] collection.
pub async fn init_indexed_multi_exchange_market_stream(
    instruments: &IndexedInstruments<Keyed<InstrumentIndex, MarketDataInstrument>>,
    sub_kinds: &[SubKind],
) -> Result<impl Stream<Item = MarketStreamEvent<InstrumentIndex, DataKind>> + use<>, DataError> {
    // Generate indexed market data Subscriptions
    let subscriptions = generate_indexed_market_data_subscription_batches(instruments, sub_kinds);

    // Initialise an indexed MarketStream via DynamicStreams
    let stream = DynamicStreams::init(subscriptions)
        .await?
        .select_all::<MarketStreamResult<InstrumentIndex, DataKind>>()
        .with_error_handler(|error| warn!(?error, "MarketStream generated error"));

    Ok(stream)
}

/// Generates batches of indexed market data `Subscriptions` from a collection of
/// `IndexedInstruments`.
///
/// This function:
/// 1. Groups instruments by [`ExchangeId`].
/// 2. Generates indexed `Subscriptions` for each Instrument-SubKind combination.
/// 4. Returns the `Subscriptions` grouped by [`ExchangeId`].
///
/// # Arguments
/// * `instruments` - Collection of `IndexedInstruments` to generate `Subscriptions` for
/// * `sub_kinds` - Slice of `SubKinds` to generate for each instrument
pub fn generate_indexed_market_data_subscription_batches(
    instruments: &IndexedInstruments<Keyed<InstrumentIndex, MarketDataInstrument>>,
    sub_kinds: &[SubKind],
) -> Vec<Vec<Subscription<ExchangeId, MarketInstrumentData<InstrumentIndex>, SubKind>>> {
    // Generate Iterator<Item = Keyed<ExchangeId, MarketInstrumentData<InstrumentIndex>>>
    let instruments = instruments.instruments().iter().map(|keyed| {
        let exchange = keyed.key.exchange;
        let instrument = MarketInstrumentData {
            key: keyed.key.clone(),
            name_exchange: jackbot_instrument::instrument::name::InstrumentNameExchange::new(
                exchange,
                keyed.value.name_exchange.clone()
            ),
            kind: keyed.value.kind,
        };
        Keyed::new(exchange, instrument)
    });

    // Chunk instruments by ExchangeId
    let instruments = instruments.sorted_unstable_by_key(|exchange| exchange.key);

    // Generate Subscriptions
    instruments
        .chunk_by(|exchange| exchange.key)
        .into_iter()
        .map(|(_exchange, instruments)| {
            instruments
                .into_iter()
                .flat_map(
                    |Keyed {
                         key: exchange,
                         value: instrument,
                     }| {
                        sub_kinds
                            .iter()
                            .map(move |kind| Subscription::new(exchange, instrument.clone(), *kind))
                    },
                )
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Indexes batches of market data `Subscriptions` using a collection of `IndexedInstruments`.
///
/// This function maps unindexed market data `Subscriptions` to indexed ones by:
/// 1. Finding the `AssetIndex` for the base and quote assets.
/// 2. Finding the `InstrumentIndex` associated with the `Subscription` `ExchangeId`, `SubKind` and
///    assets.
/// 3. Creating new `Subscriptions` with indexed instruments.
///
///
/// # Arguments
/// * `instruments` - Collection of `IndexedInstruments` used for indexing
/// * `batches` - Iterator of `Subscription` batches to be indexed
pub fn index_market_data_subscription_batches<SubBatchIter, SubIter, Sub>(
    instruments: &IndexedInstruments<Keyed<InstrumentIndex, MarketDataInstrument>>,
    batches: SubBatchIter,
) -> Result<
    Vec<Vec<Subscription<ExchangeId, Keyed<InstrumentIndex, MarketDataInstrument>>>>,
    DataError,
>
where
    SubBatchIter: IntoIterator<Item = SubIter>,
    SubIter: IntoIterator<Item = Sub>,
    Sub: Into<Subscription<ExchangeId, MarketDataInstrument, SubKind>>,
{
    batches
        .into_iter()
        .map(|batch| batch
            .into_iter()
            .map(|sub| {
                let sub = sub.into();

                // TODO: Implement asset index lookup if needed
                // let base_index = instruments.find_asset_index(sub.exchange, &sub.instrument.instrument.base)?;
                // let quote_index = instruments.find_asset_index(sub.exchange, &sub.instrument.instrument.quote)?;

                let find_instrument = |exchange: &ExchangeId, kind: &jackbot_instrument::instrument::market_data::kind::MarketDataInstrumentKind, base: &str, quote: &str| {
                    instruments
                        .instruments()
                        .iter()
                        .find_map(|indexed| {
                            (
                                indexed.key.exchange == *exchange
                                    && indexed.value.instrument.kind == *kind
                                    && indexed.value.instrument.base.0 == *base
                                    && indexed.value.instrument.quote.0 == *quote
                            ).then_some(indexed.key.clone())
                        })
                        .ok_or_else(|| IndexError::new(format!(
                            "Instrument: ({}, {}, {}, {}) must be present in indexed instruments: {:?}",
                            exchange, kind, base, quote, instruments.instruments()
                        )))
                };

                let instrument_index = find_instrument(
                    &sub.exchange, 
                    &sub.instrument.instrument.kind, 
                    &sub.instrument.instrument.base.0, 
                    &sub.instrument.instrument.quote.0
                )?;

                Ok(Subscription {
                    exchange: sub.exchange,
                    instrument: Keyed::new(instrument_index, sub.instrument),
                    kind: sub.kind,
                })
            })
            .collect()
        )
        .collect()
}
