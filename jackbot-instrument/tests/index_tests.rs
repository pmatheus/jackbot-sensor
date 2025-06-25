use jackbot_instrument::{
    Keyed, Underlying,
    asset::{
        Asset,
        name::{AssetNameExchange, AssetNameInternal},
    },
    exchange::ExchangeId,
    index::{IndexError, IndexedInstruments},
    instrument::{
        Instrument,
        kind::InstrumentKind,
        name::{InstrumentNameExchange, InstrumentNameInternal},
        quote::InstrumentQuoteAsset,
    },
};

// Copied from jackbot-instrument/src/lib.rs test_utils
// Consider moving this to a shared test utility crate or module if used across multiple test files
fn exchange_asset(
    exchange: ExchangeId,
    symbol: &str,
) -> jackbot_instrument::asset::ExchangeAsset<Asset> {
    jackbot_instrument::asset::ExchangeAsset {
        exchange,
        asset: asset(symbol),
    }
}

fn asset(symbol: &str) -> Asset {
    Asset {
        name_internal: AssetNameInternal::from(symbol),
        name_exchange: AssetNameExchange::from(symbol),
    }
}

fn instrument(exchange: ExchangeId, base: &str, quote: &str) -> Instrument<ExchangeId, Asset> {
    let name_exchange = InstrumentNameExchange::from(format!("{base}_{quote}"));
    let name_internal = InstrumentNameInternal::new_from_exchange(exchange, name_exchange.clone());
    let base_asset = asset(base);
    let quote_asset = asset(quote);

    Instrument::new(
        exchange,
        name_internal,
        name_exchange,
        Underlying::new(base_asset, quote_asset),
        InstrumentQuoteAsset::UnderlyingQuote,
        InstrumentKind::Spot,
        None,
    )
}

#[test]
fn test_indexed_instruments_new() {
    let instruments = vec![
        instrument(ExchangeId::BinanceSpot, "btc", "usdt"),
        instrument(ExchangeId::BinanceSpot, "eth", "usdt"),
        instrument(ExchangeId::Kraken, "xbt", "usd"),
    ];

    let indexed = IndexedInstruments::new(instruments);

    assert_eq!(indexed.exchanges().len(), 2);
    assert_eq!(indexed.assets().len(), 5); // btc, usdt (Binance), eth (Binance), xbt, usd (Kraken)
    assert_eq!(indexed.instruments().len(), 3);
}

#[test]
fn test_indexed_instruments_builder() {
    let indexed = IndexedInstruments::builder()
        .add_instrument(instrument(ExchangeId::BinanceSpot, "btc", "usdt"))
        .add_instrument(instrument(ExchangeId::BinanceSpot, "eth", "usdt"))
        .add_instrument(instrument(ExchangeId::Kraken, "xbt", "usd"))
        .build();

    assert_eq!(indexed.exchanges().len(), 2);
    assert_eq!(indexed.assets().len(), 5); // btc, usdt (Binance), eth (Binance), xbt, usd (Kraken)
    assert_eq!(indexed.instruments().len(), 3);
}

#[test]
fn test_find_exchange_index() {
    let indexed = IndexedInstruments::builder()
        .add_instrument(instrument(ExchangeId::BinanceSpot, "btc", "usdt"))
        .build();

    let exchange_index = indexed
        .find_exchange_index(ExchangeId::BinanceSpot)
        .unwrap();
    assert_eq!(
        indexed.find_exchange(exchange_index).unwrap(),
        ExchangeId::BinanceSpot
    );

    let err = indexed.find_exchange_index(ExchangeId::Kraken).unwrap_err();
    assert!(matches!(err, IndexError::ExchangeIndex(_)));
}

#[test]
fn test_find_asset_index() {
    let indexed = IndexedInstruments::builder()
        .add_instrument(instrument(ExchangeId::BinanceSpot, "btc", "usdt"))
        .build();

    let asset_index = indexed
        .find_asset_index(ExchangeId::BinanceSpot, &AssetNameInternal::from("btc"))
        .unwrap();
    let found_asset = indexed.find_asset(asset_index).unwrap();
    assert_eq!(found_asset.exchange, ExchangeId::BinanceSpot);
    assert_eq!(
        found_asset.asset.name_internal,
        AssetNameInternal::from("btc")
    );

    let err = indexed
        .find_asset_index(ExchangeId::Kraken, &AssetNameInternal::from("xbt"))
        .unwrap_err();
    assert!(matches!(err, IndexError::AssetIndex(_)));
}

#[test]
fn test_find_instrument_index() {
    let btc_usdt_binance = instrument(ExchangeId::BinanceSpot, "btc", "usdt");
    let indexed = IndexedInstruments::builder()
        .add_instrument(btc_usdt_binance.clone())
        .build();

    let instrument_index = indexed
        .find_instrument_index(ExchangeId::BinanceSpot, &btc_usdt_binance.name_internal)
        .unwrap();

    let found_instrument = indexed.find_instrument(instrument_index).unwrap();
    assert_eq!(
        found_instrument.name_internal,
        btc_usdt_binance.name_internal
    );

    let eth_usdt_name = InstrumentNameInternal::new_from_exchange(
        ExchangeId::BinanceSpot,
        InstrumentNameExchange::from("eth_usdt"),
    );
    let err = indexed
        .find_instrument_index(ExchangeId::BinanceSpot, &eth_usdt_name)
        .unwrap_err();
    assert!(matches!(err, IndexError::InstrumentIndex(_)));
}

#[test]
fn test_from_iter() {
    let instruments = vec![
        instrument(ExchangeId::BinanceSpot, "btc", "usdt"),
        instrument(ExchangeId::BinanceSpot, "eth", "usdt"),
        instrument(ExchangeId::Kraken, "xbt", "usd"),
    ];

    let indexed: IndexedInstruments = instruments.into_iter().collect();

    assert_eq!(indexed.exchanges().len(), 2);
    assert_eq!(indexed.assets().len(), 5); // btc, usdt (Binance), eth (Binance), xbt, usd (Kraken)
    assert_eq!(indexed.instruments().len(), 3);
}

// Test for find_exchange_by_exchange_id helper
#[test]
fn test_find_exchange_by_exchange_id_util() {
    let exchanges = vec![
        Keyed::new(
            jackbot_instrument::exchange::ExchangeIndex(0),
            ExchangeId::BinanceSpot,
        ),
        Keyed::new(
            jackbot_instrument::exchange::ExchangeIndex(1),
            ExchangeId::Kraken,
        ),
    ];
    assert_eq!(
        jackbot_instrument::index::find_exchange_by_exchange_id(
            &exchanges,
            &ExchangeId::BinanceSpot
        )
        .unwrap(),
        jackbot_instrument::exchange::ExchangeIndex(0)
    );
}

// Test for find_asset_by_exchange_and_name_internal helper
#[test]
fn test_find_asset_by_exchange_and_name_internal_util() {
    let assets = vec![
        Keyed::new(
            jackbot_instrument::asset::AssetIndex(0),
            exchange_asset(ExchangeId::BinanceSpot, "btc"),
        ),
        Keyed::new(
            jackbot_instrument::asset::AssetIndex(1),
            exchange_asset(ExchangeId::BinanceSpot, "usdt"),
        ),
        Keyed::new(
            jackbot_instrument::asset::AssetIndex(2),
            exchange_asset(ExchangeId::Kraken, "xbt"),
        ),
    ];
    assert_eq!(
        jackbot_instrument::index::find_asset_by_exchange_and_name_internal(
            &assets,
            ExchangeId::BinanceSpot,
            &AssetNameInternal::from("btc")
        )
        .unwrap(),
        jackbot_instrument::asset::AssetIndex(0)
    );
    assert!(
        jackbot_instrument::index::find_asset_by_exchange_and_name_internal(
            &assets,
            ExchangeId::BinanceSpot,
            &AssetNameInternal::from("eth")
        )
        .is_err()
    );
}
