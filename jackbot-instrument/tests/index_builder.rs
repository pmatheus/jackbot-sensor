#![allow(unused_crate_dependencies)]
use jackbot_instrument::{
    Underlying, // Corrected path
    asset::Asset,
    index::ExchangeId,
    index::{AssetIndex, ExchangeIndex, Keyed, builder::IndexedInstrumentsBuilder},
    instrument::kind::InstrumentKind,
    instrument::{
        Instrument,
        name::{InstrumentNameExchange, InstrumentNameInternal},
        quote::InstrumentQuoteAsset,
        spec::{
            InstrumentSpec,
            InstrumentSpecNotional,
            InstrumentSpecPrice,
            InstrumentSpecQuantity,
            OrderQuantityUnits, // Corrected path
        },
    },
    test_utils::{exchange_asset, instrument},
};
#[allow(unused_imports)]
use rust_decimal_macros::dec; // Added import for dec! macro

#[test]
fn test_builder_basic_spot() {
    // Add single spot instrument
    let indexed = IndexedInstrumentsBuilder::default()
        .add_instrument(instrument(ExchangeId::BinanceSpot, "btc", "usdt"))
        .build();

    // Verify state
    assert_eq!(indexed.exchanges().len(), 1);
    assert_eq!(indexed.assets().len(), 2); // BTC and USDT
    assert_eq!(indexed.instruments().len(), 1);

    // Verify exchanges indexes
    assert_eq!(indexed.exchanges()[0].value, ExchangeId::BinanceSpot);

    // Verify asset indexes
    assert_eq!(
        indexed.assets()[0].value,
        exchange_asset(ExchangeId::BinanceSpot, "btc"),
    );
    assert_eq!(
        indexed.assets()[1].value,
        exchange_asset(ExchangeId::BinanceSpot, "usdt"),
    );

    // Verify instrument indexes
    assert_eq!(
        indexed.instruments()[0].value,
        Instrument {
            exchange: Keyed::new(ExchangeIndex(0), ExchangeId::BinanceSpot),
            name_exchange: InstrumentNameExchange::new("btc_usdt"),
            name_internal: InstrumentNameInternal::new("binance_spot-btc_usdt"),
            underlying: Underlying {
                base: AssetIndex(0),
                quote: AssetIndex(1),
            },
            quote: InstrumentQuoteAsset::UnderlyingQuote,
            kind: InstrumentKind::Spot,
            spec: None
        }
    );
}

#[test]
fn test_builder_deduplication() {
    // Add same spot instrument twice
    let indexed = IndexedInstrumentsBuilder::default()
        .add_instrument(instrument(ExchangeId::BinanceSpot, "BTC", "USDT"))
        .add_instrument(instrument(ExchangeId::BinanceSpot, "BTC", "USDT"))
        .build();

    // Should deduplicate exchanges and assets, but not instruments
    assert_eq!(indexed.exchanges().len(), 1); // Exchange are de-duped
    assert_eq!(indexed.assets().len(), 2); // BTC and USDT and de-duped
    assert_eq!(indexed.instruments().len(), 1); // Instruments are de-duped
}

#[test]
fn test_builder_multiple_exchanges() {
    // Add instruments from different exchanges
    let indexed = IndexedInstrumentsBuilder::default()
        .add_instrument(instrument(ExchangeId::BinanceSpot, "BTC", "USDT"))
        .add_instrument(instrument(ExchangeId::Coinbase, "BTC", "USD"))
        .build();

    // Should maintain separate indices for same asset on different exchanges
    assert_eq!(indexed.exchanges().len(), 2);
    assert_eq!(indexed.assets().len(), 4); // BTC on both exchanges, USDT and USD
    assert_eq!(indexed.instruments().len(), 2);
}

#[test]
fn test_builder_asset_unit_handling() {
    // Create instrument with asset-based order quantity
    let base_asset = Asset::new_from_exchange("BTC");
    let quote_asset = Asset::new_from_exchange("USDT");

    let instrument = Instrument::new(
        ExchangeId::BinanceSpot,
        "binance_spot_btc_usdt",
        "BTC-USDT",
        Underlying::new(base_asset.clone(), quote_asset.clone()),
        InstrumentQuoteAsset::UnderlyingQuote,
        InstrumentKind::Spot,
        Some(InstrumentSpec {
            price: InstrumentSpecPrice {
                min: dec!(0.1),
                tick_size: dec!(0.1),
            },
            quantity: InstrumentSpecQuantity {
                unit: OrderQuantityUnits::Asset(base_asset.clone()),
                min: dec!(0.001),
                increment: dec!(0.001),
            },
            notional: InstrumentSpecNotional { min: dec!(10) },
        }),
    );

    let indexed = IndexedInstrumentsBuilder::default()
        .add_instrument(instrument)
        .build();

    // Should index the asset used in OrderQuantityUnits
    assert_eq!(indexed.assets().len(), 2);
    assert_eq!(
        indexed.assets()[0].value,
        exchange_asset(ExchangeId::BinanceSpot, "BTC")
    );
}

#[test]
fn test_builder_ordering() {
    // Add instruments in any order
    let indexed = IndexedInstrumentsBuilder::default()
        .add_instrument(instrument(ExchangeId::BinanceSpot, "BTC", "USDT"))
        .add_instrument(instrument(ExchangeId::Coinbase, "ETH", "USD"))
        .build();

    // Verify exchanges are ordered by input sequence
    assert_eq!(indexed.exchanges()[0].value, ExchangeId::BinanceSpot);
    assert_eq!(indexed.exchanges()[1].value, ExchangeId::Coinbase);

    // Verify exchanges are ordered by input sequence
    assert_eq!(
        indexed.assets()[0].value,
        exchange_asset(ExchangeId::BinanceSpot, "BTC")
    );
    assert_eq!(
        indexed.assets()[1].value,
        exchange_asset(ExchangeId::BinanceSpot, "USDT")
    );
    assert_eq!(
        indexed.assets()[2].value,
        exchange_asset(ExchangeId::Coinbase, "ETH")
    );
    assert_eq!(
        indexed.assets()[3].value,
        exchange_asset(ExchangeId::Coinbase, "USD")
    );

    // Verify instruments are ordered by input sequence
    assert_eq!(
        indexed.instruments()[0].value,
        Instrument {
            exchange: Keyed::new(ExchangeIndex(0), ExchangeId::BinanceSpot),
            name_exchange: InstrumentNameExchange::new("BTC_USDT"),
            name_internal: InstrumentNameInternal::new("binance_spot-btc_usdt"),
            underlying: Underlying {
                base: AssetIndex(0),
                quote: AssetIndex(1),
            },
            quote: InstrumentQuoteAsset::UnderlyingQuote,
            kind: InstrumentKind::Spot,
            spec: None
        }
    );

    assert_eq!(
        indexed.instruments()[1].value,
        Instrument {
            exchange: Keyed::new(ExchangeIndex(1), ExchangeId::Coinbase),
            name_exchange: InstrumentNameExchange::new("ETH_USD"),
            name_internal: InstrumentNameInternal::new("coinbase-eth_usd"),
            underlying: Underlying {
                base: AssetIndex(2),
                quote: AssetIndex(3),
            },
            quote: InstrumentQuoteAsset::UnderlyingQuote,
            kind: InstrumentKind::Spot,
            spec: None
        }
    );
}
