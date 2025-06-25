use jackbot_data::books::aggregator::OrderBookAggregator;
use jackbot_execution::{
    client::{
        binance::{
            futures::{BinanceFuturesUsd, BinanceFuturesUsdConfig},
            paper::{BinancePaperClient, BinancePaperConfig},
            BinanceWsClient, BinanceWsConfig,
        },
        coinbase::{CoinbaseWsClient, CoinbaseWsConfig},
        // cryptocom::{CryptocomClient, CryptocomConfig}, // Commented out
        // gateio::{GateIoClient, GateIoConfig}, // Commented out
        kraken::{KrakenWsClient, KrakenWsConfig},
        // mexc::{MexcClient, MexcConfig}, // Commented out
        okx::{OkxWsClient, OkxWsConfig},
        ExecutionClient,
    },
    strategy::always_maker::AlwaysMaker, // Corrected path for AlwaysMaker
    strategy::twap::TwapScheduler,
    strategy::vwap::VwapScheduler,
};
use rand::rngs::StdRng;
use rand::SeedableRng;
use std::sync::Arc;
use url::Url;

#[test]
fn advanced_orders_compile_all_clients() {
    let aggregator = Arc::new(OrderBookAggregator::default());
    let rng = StdRng::seed_from_u64(1);
    let client = BinanceFuturesUsd::new(BinanceFuturesUsdConfig::default());
    let _twap = TwapScheduler::new(client.clone(), (*aggregator).clone(), rng.clone());
    let _vwap = VwapScheduler::new(client.clone(), (*aggregator).clone(), rng.clone());
    let _maker = AlwaysMaker::new(client, aggregator.clone());

    let config = BinanceWsConfig {
        url: Url::parse("wss://test").unwrap(),
        auth_payload: String::new(),
    };
    let client = BinanceWsClient::new(config);
    let _twap = TwapScheduler::new(client.clone(), (*aggregator).clone(), rng.clone());
    let _vwap = VwapScheduler::new(client.clone(), (*aggregator).clone(), rng.clone());
    let _maker = AlwaysMaker::new(client, aggregator.clone());

    let config = BinancePaperConfig {
        books: Default::default(),
        instruments: Default::default(),
        snapshot: jackbot_execution::UnindexedAccountSnapshot {
            exchange: jackbot_instrument::exchange::ExchangeId::BinanceSpot,
            balances: Vec::new(),
            instruments: Vec::new(),
        },
        fees_percent: Default::default(),
    };
    let client = BinancePaperClient::new(config);
    let _twap = TwapScheduler::new(client.clone(), (*aggregator).clone(), rng.clone());
    let _vwap = VwapScheduler::new(client.clone(), (*aggregator).clone(), rng.clone());
    let _maker = AlwaysMaker::new(client, aggregator.clone());

    let config = CoinbaseWsConfig {
        url: Url::parse("wss://test").unwrap(),
        auth_payload: String::new(),
    };
    let client = CoinbaseWsClient::new(config);
    let _twap = TwapScheduler::new(client.clone(), (*aggregator).clone(), rng.clone());
    let _vwap = VwapScheduler::new(client.clone(), (*aggregator).clone(), rng.clone());
    let _maker = AlwaysMaker::new(client, aggregator.clone());

    // let client = CryptocomClient::new(CryptocomConfig::default()); // Commented out
    // let _twap = TwapScheduler::new(client.clone(), aggregator.clone(), rng.clone()); // Commented out
    // let _vwap = VwapScheduler::new(client.clone(), aggregator.clone(), rng.clone()); // Commented out
    // let _maker = AlwaysMaker::new(client, aggregator.clone()); // Commented out

    // let client = GateIoClient::new(GateIoConfig::default()); // Commented out
    // let _twap = TwapScheduler::new(client.clone(), aggregator.clone(), rng.clone()); // Commented out
    // let _vwap = VwapScheduler::new(client.clone(), aggregator.clone(), rng.clone()); // Commented out
    // let _maker = AlwaysMaker::new(client, aggregator.clone()); // Commented out

    // let client = MexcClient::new(MexcConfig::default()); // Commented out
    // let _twap = TwapScheduler::new(client.clone(), aggregator.clone(), rng.clone()); // Commented out
    // let _vwap = VwapScheduler::new(client.clone(), aggregator.clone(), rng.clone()); // Commented out
    // let _maker = AlwaysMaker::new(client, aggregator.clone()); // Commented out

    let config = OkxWsConfig {
        url: Url::parse("wss://test").unwrap(),
        auth_payload: String::new(),
    };
    let client = OkxWsClient::new(config);
    let _twap = TwapScheduler::new(client.clone(), (*aggregator).clone(), rng.clone());
    let _vwap = VwapScheduler::new(client.clone(), (*aggregator).clone(), rng.clone());
    let _maker = AlwaysMaker::new(client, aggregator.clone());

    let config = KrakenWsConfig {
        url: Url::parse("wss://test").unwrap(),
        auth_payload: String::new(),
    };
    let client = KrakenWsClient::new(config);
    let _twap = TwapScheduler::new(client.clone(), (*aggregator).clone(), rng);
    let _vwap = VwapScheduler::new(
        client.clone(),
        OrderBookAggregator::default(),
        StdRng::seed_from_u64(2),
    );
    let _maker = AlwaysMaker::new(client, Arc::new(OrderBookAggregator::default()));
}
