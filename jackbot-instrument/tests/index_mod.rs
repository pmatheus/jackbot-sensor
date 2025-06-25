use jackbot_instrument::{
    Keyed,
    Underlying,
    asset::name::AssetNameInternal, // Corrected import path
    asset::{Asset, ExchangeAsset},
    index::AssetIndex,                               // Corrected import path
    index::ExchangeId,                               // Corrected import path
    index::ExchangeIndex,                            // Corrected import path
    index::find_asset_by_exchange_and_name_internal, // Corrected import path
    index::find_exchange_by_exchange_id,             // Corrected import path
    index::{IndexedInstruments, error::IndexError},
    instrument::kind::InstrumentKind, // Corrected import path
    instrument::{
        Instrument,
        name::{InstrumentNameExchange, InstrumentNameInternal},
        quote::InstrumentQuoteAsset,
    },
    test_utils::{exchange_asset, instrument},
};

#[test]
fn test_indexed_instruments_new() {
    // Test creating empty IndexedInstruments
    let empty = IndexedInstruments::new(std::iter::empty::<Instrument<ExchangeId, Asset>>());
    assert!(empty.exchanges().is_empty());
    assert!(empty.assets().is_empty());
    assert!(empty.instruments().is_empty());

    // Test creating with single instrument
    let instrument = instrument(ExchangeId::BinanceSpot, "btc", "usdt");
    let actual = IndexedInstruments::new(std::iter::once(instrument));

    assert_eq!(actual.exchanges().len(), 1);
    assert_eq!(actual.assets().len(), 2); // BTC and USDT
    assert_eq!(actual.instruments().len(), 1);

    // Verify exchanges indexes
    assert_eq!(actual.exchanges()[0].value, ExchangeId::BinanceSpot);

    // Verify asset indexes
    assert_eq!(
        actual.assets()[0].value,
        exchange_asset(ExchangeId::BinanceSpot, "btc"),
    );
    assert_eq!(
        actual.assets()[1].value,
        exchange_asset(ExchangeId::BinanceSpot, "usdt"),
    );

    // Very instrument indexes
    assert_eq!(
        actual.instruments()[0].value,
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
fn test_indexed_instruments_multiple() {
    let instruments = vec![
        instrument(ExchangeId::BinanceSpot, "BTC", "USDT"),
        instrument(ExchangeId::BinanceSpot, "ETH", "USDT"),
        instrument(ExchangeId::Coinbase, "BTC", "USD"),
    ];

    let indexed = IndexedInstruments::new(instruments);

    // Should have 2 exchanges, 4 assets (BTC, ETH, USDT, USD), and 3 instruments
    assert_eq!(indexed.exchanges().len(), 2);
    assert_eq!(indexed.assets().len(), 5);
    assert_eq!(indexed.instruments().len(), 3);

    // Verify exchanges
    let exchanges: Vec<_> = indexed.exchanges().iter().map(|e| e.value).collect();
    assert!(exchanges.contains(&ExchangeId::BinanceSpot));
    assert!(exchanges.contains(&ExchangeId::Coinbase));
}

#[test]
fn test_find_exchange_index() {
    let instruments = vec![
        instrument(ExchangeId::BinanceSpot, "BTC", "USDT"),
        instrument(ExchangeId::Coinbase, "ETH", "USD"),
    ];
    let indexed = IndexedInstruments::new(instruments);

    // Test finding existing exchanges
    assert!(indexed.find_exchange_index(ExchangeId::BinanceSpot).is_ok());
    assert!(indexed.find_exchange_index(ExchangeId::Coinbase).is_ok());

    // Test finding non-existent exchange
    let err = indexed.find_exchange_index(ExchangeId::Kraken).unwrap_err();
    assert!(matches!(err, IndexError::ExchangeIndex(_)));
}

#[test]
fn test_find_asset_index() {
    let instruments = vec![
        instrument(ExchangeId::BinanceSpot, "BTC", "USDT"),
        instrument(ExchangeId::Coinbase, "ETH", "USD"),
    ];
    let indexed = IndexedInstruments::new(instruments);

    // Test finding existing assets
    assert!(
        indexed
            .find_asset_index(ExchangeId::BinanceSpot, &AssetNameInternal::from("btc"))
            .is_ok()
    );
    assert!(
        indexed
            .find_asset_index(ExchangeId::BinanceSpot, &AssetNameInternal::from("usdt"))
            .is_ok()
    );
    assert!(
        indexed
            .find_asset_index(ExchangeId::Coinbase, &AssetNameInternal::from("eth"))
            .is_ok()
    );

    // Test finding asset with wrong exchange
    let err = indexed
        .find_asset_index(ExchangeId::Kraken, &AssetNameInternal::from("btc"))
        .unwrap_err();
    assert!(matches!(err, IndexError::AssetIndex(_)));

    // Test finding non-existent asset
    let err = indexed
        .find_asset_index(
            ExchangeId::BinanceSpot,
            &AssetNameInternal::from("nonexistent"),
        )
        .unwrap_err();
    assert!(matches!(err, IndexError::AssetIndex(_)));
}

#[test]
fn test_find_instrument_index() {
    let instruments = vec![
        instrument(ExchangeId::BinanceSpot, "btc", "usdt"),
        instrument(ExchangeId::Coinbase, "eth", "usd"),
    ];

    let indexed = IndexedInstruments::new(instruments);
    let btc_usdt = InstrumentNameInternal::from("binance_spot-btc_usdt");

    // Test finding existing instruments
    assert!(
        indexed
            .find_instrument_index(ExchangeId::BinanceSpot, &btc_usdt)
            .is_ok()
    );

    // Test finding instrument with wrong exchange
    let err = indexed
        .find_instrument_index(ExchangeId::Kraken, &btc_usdt)
        .unwrap_err();
    assert!(matches!(err, IndexError::InstrumentIndex(_)));

    // Test finding non-existent instrument
    let nonexistent = InstrumentNameInternal::from("nonexistent");
    let err = indexed
        .find_instrument_index(ExchangeId::BinanceSpot, &nonexistent)
        .unwrap_err();
    assert!(matches!(err, IndexError::InstrumentIndex(_)));
}

#[test]
fn test_private_find_exchange_by_exchange_id() {
    let exchanges = vec![
        Keyed {
            key: ExchangeIndex(0),
            value: ExchangeId::BinanceSpot,
        },
        Keyed {
            key: ExchangeIndex(1),
            value: ExchangeId::Coinbase,
        },
    ];

    // Test finding existing exchange
    let result = find_exchange_by_exchange_id(&exchanges, &ExchangeId::BinanceSpot);
    assert_eq!(result.unwrap(), ExchangeIndex(0));

    // Test finding non-existent exchange
    let err = find_exchange_by_exchange_id(&exchanges, &ExchangeId::Kraken).unwrap_err();
    assert!(matches!(err, IndexError::ExchangeIndex(_)));
}

#[test]
fn test_private_find_asset_by_exchange_and_name_internal() {
    let assets = vec![
        Keyed {
            key: AssetIndex(0),
            value: ExchangeAsset {
                exchange: ExchangeId::BinanceSpot,
                asset: Asset::new_from_exchange("BTC"),
            },
        },
        Keyed {
            key: AssetIndex(1),
            value: ExchangeAsset {
                exchange: ExchangeId::BinanceSpot,
                asset: Asset::new_from_exchange("USDT"),
            },
        },
    ];

    // Test finding existing asset
    let result = find_asset_by_exchange_and_name_internal(
        &assets,
        ExchangeId::BinanceSpot,
        &AssetNameInternal::from("btc"),
    );
    assert_eq!(result.unwrap(), AssetIndex(0));

    // Test finding asset with wrong exchange
    let err = find_asset_by_exchange_and_name_internal(
        &assets,
        ExchangeId::Kraken,
        &AssetNameInternal::from("btc"),
    )
    .unwrap_err();
    assert!(matches!(err, IndexError::AssetIndex(_)));

    // Test finding non-existent asset
    let err = find_asset_by_exchange_and_name_internal(
        &assets,
        ExchangeId::BinanceSpot,
        &AssetNameInternal::from("nonexistent"),
    )
    .unwrap_err();
    assert!(matches!(err, IndexError::AssetIndex(_)));
}

#[test]
fn test_duplicates_are_filtered_correctly() {
    // Test with duplicate instruments
    let instruments = vec![
        instrument(ExchangeId::BinanceSpot, "btc", "usdt"),
        instrument(ExchangeId::BinanceSpot, "btc", "usdt"),
    ];
    let indexed = IndexedInstruments::new(instruments);

    // Should deduplicate exchanges and assets
    assert_eq!(indexed.exchanges().len(), 1);
    assert_eq!(indexed.assets().len(), 2);
    assert_eq!(indexed.instruments().len(), 1); // Instruments aren't deduplicated

    // Test with same asset on different exchanges
    let instruments = vec![
        instrument(ExchangeId::BinanceSpot, "btc", "usdt"),
        instrument(ExchangeId::Coinbase, "btc", "usdt"),
    ];
    let indexed = IndexedInstruments::new(instruments);

    // Should have separate entries for same asset on different exchanges
    assert_eq!(indexed.exchanges().len(), 2);
    assert_eq!(indexed.assets().len(), 4); // BTC and USDT on both exchanges
    assert_eq!(indexed.instruments().len(), 2);
}
