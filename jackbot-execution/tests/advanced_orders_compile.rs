use jackbot_execution::{
    always_maker::AlwaysMaker,
    twap::TwapScheduler,
    vwap::VwapScheduler,
    client::{
        binance::{futures::{BinanceFuturesUsd, BinanceFuturesUsdConfig}, mod::{BinanceWsClient, BinanceWsConfig}, paper::{BinancePaperClient, BinancePaperConfig}},
        coinbase::{CoinbaseWsClient, CoinbaseWsConfig},
        cryptocom::{CryptocomClient, CryptocomConfig},
        gateio::{GateIoClient, GateIoConfig},
        mexc::{MexcClient, MexcConfig},
        okx::{OkxWsClient, OkxWsConfig},
        kraken::{KrakenWsClient, KrakenWsConfig},
    },
};
use jackbot_data::books::aggregator::OrderBookAggregator;
use rand::rngs::StdRng;
use rand::SeedableRng;
use url::Url;

#[test]
fn advanced_orders_compile_all_clients() {
    let aggregator = OrderBookAggregator::default();
    let rng = StdRng::seed_from_u64(1);
    let client = BinanceFuturesUsd::new(BinanceFuturesUsdConfig::default());
    let _twap = TwapScheduler::new(client.clone(), aggregator.clone(), rng.clone());
    let _vwap = VwapScheduler::new(client.clone(), aggregator.clone(), rng.clone());
    let _maker = AlwaysMaker::new(client, aggregator.clone());

    let config = BinanceWsConfig { url: Url::parse("wss://test").unwrap(), auth_payload: String::new() };
    let client = BinanceWsClient::new(config);
    let _twap = TwapScheduler::new(client.clone(), aggregator.clone(), rng.clone());
    let _vwap = VwapScheduler::new(client.clone(), aggregator.clone(), rng.clone());
    let _maker = AlwaysMaker::new(client, aggregator.clone());

    let config = BinancePaperConfig { books: Default::default(), instruments: Default::default(), snapshot: jackbot_execution::UnindexedAccountSnapshot { exchange: jackbot_instrument::exchange::ExchangeId::BinanceSpot, balances: Vec::new(), instruments: Vec::new() }, fees_percent: Default::default() };
    let client = BinancePaperClient::new(config);
    let _twap = TwapScheduler::new(client.clone(), aggregator.clone(), rng.clone());
    let _vwap = VwapScheduler::new(client.clone(), aggregator.clone(), rng.clone());
    let _maker = AlwaysMaker::new(client, aggregator.clone());

    let config = CoinbaseWsConfig { url: Url::parse("wss://test").unwrap(), auth_payload: String::new() };
    let client = CoinbaseWsClient::new(config);
    let _twap = TwapScheduler::new(client.clone(), aggregator.clone(), rng.clone());
    let _vwap = VwapScheduler::new(client.clone(), aggregator.clone(), rng.clone());
    let _maker = AlwaysMaker::new(client, aggregator.clone());

    let client = CryptocomClient::new(CryptocomConfig::default());
    let _twap = TwapScheduler::new(client.clone(), aggregator.clone(), rng.clone());
    let _vwap = VwapScheduler::new(client.clone(), aggregator.clone(), rng.clone());
    let _maker = AlwaysMaker::new(client, aggregator.clone());

    let client = GateIoClient::new(GateIoConfig::default());
    let _twap = TwapScheduler::new(client.clone(), aggregator.clone(), rng.clone());
    let _vwap = VwapScheduler::new(client.clone(), aggregator.clone(), rng.clone());
    let _maker = AlwaysMaker::new(client, aggregator.clone());

    let client = MexcClient::new(MexcConfig::default());
    let _twap = TwapScheduler::new(client.clone(), aggregator.clone(), rng.clone());
    let _vwap = VwapScheduler::new(client.clone(), aggregator.clone(), rng.clone());
    let _maker = AlwaysMaker::new(client, aggregator.clone());

    let config = OkxWsConfig { url: Url::parse("wss://test").unwrap(), auth_payload: String::new() };
    let client = OkxWsClient::new(config);
    let _twap = TwapScheduler::new(client.clone(), aggregator.clone(), rng.clone());
    let _vwap = VwapScheduler::new(client.clone(), aggregator.clone(), rng.clone());
    let _maker = AlwaysMaker::new(client, aggregator.clone());

    let config = KrakenWsConfig { url: Url::parse("wss://test").unwrap(), auth_payload: String::new() };
    let client = KrakenWsClient::new(config);
    let _twap = TwapScheduler::new(client.clone(), aggregator, rng);
    let _vwap = VwapScheduler::new(client.clone(), OrderBookAggregator::default(), StdRng::seed_from_u64(2));
    let _maker = AlwaysMaker::new(client, OrderBookAggregator::default());
}
