# Trade WebSocket APIs

This document provides details on real-time trade streams for all exchanges supported by Jackbot. Each section outlines the WebSocket URL, subscription structure and reference links for spot and futures markets.

## Quick Reference

### Spot Markets
| Exchange | WebSocket Endpoint |
|----------|-------------------|
| Binance Spot | `wss://stream.binance.com:9443/ws` |
| Bitget Spot | `wss://ws.bitget.com/spot/v1/stream` |
| Bybit Spot | `wss://stream.bybit.com/v5/public/spot` |
| Coinbase | `wss://ws-feed.exchange.coinbase.com` |
| Kraken Spot | `wss://ws.kraken.com/` |
| Kucoin Spot | `wss://ws-api.kucoin.com/endpoint` |
| OKX Spot | `wss://ws.okx.com:8443/ws/v5/public` |
| Gate.io Spot | `wss://api.gateio.ws/ws/v4/` |
| Crypto.com Spot | `wss://stream.crypto.com/v2/market` |
| MEXC Spot | `wss://wbs.mexc.com/ws` |
| Hyperliquid Spot | `wss://api.hyperliquid.xyz/ws` |

### Futures Markets
| Exchange | WebSocket Endpoint |
|----------|-------------------|
| Binance Futures | `wss://fstream.binance.com/ws` |
| Bitget Futures | `wss://ws.bitget.com/mix/v1/stream` |
| Bybit Futures | `wss://stream.bybit.com/v5/public/linear` |
| Kraken Futures | `wss://futures.kraken.com/ws/v1` |
| Kucoin Futures | `wss://ws-api-futures.kucoin.com/endpoint` |
| OKX Futures | `wss://ws.okx.com:8443/ws/v5/public` |
| Gate.io Futures | `wss://fx-ws.gateio.ws/v4/ws/` |
| Crypto.com Futures | `wss://deriv-stream.crypto.com/v1/market` |
| MEXC Futures | `wss://contract.mexc.com/ws` |
| Hyperliquid Futures | `wss://api.hyperliquid.xyz/ws` |

## Exchange Details

### Binance

#### Spot Market
- **WebSocket URL**: wss://stream.binance.com:9443/ws
- **Subscription Format**:
  ```json
  {
    "method": "SUBSCRIBE",
    "params": ["<symbol>@trade"],
    "id": 1
  }
  ```
- **Data Fields**: `price`, `quantity`, `side`, `trade id`, `timestamp`
- **Reference**: [Binance WebSocket API Documentation](https://binance-docs.github.io/apidocs/spot/en/#trade-streams)

#### Futures Market
- **WebSocket URL**: wss://fstream.binance.com/ws
- **Subscription Format**: same as spot using the futures endpoint
- **Reference**: [Binance Futures WebSocket Documentation](https://binance-docs.github.io/apidocs/futures/en/#trade-stream)

### Bitget

#### Spot Market
- **WebSocket URL**: wss://ws.bitget.com/spot/v1/stream
- **Subscription Format**:
  ```json
  {
    "op": "subscribe",
    "args": [{"channel": "trade", "instId": "<symbol>"}]
  }
  ```
- **Reference**: [Bitget Spot WebSocket Docs](https://bitgetlimited.github.io/apidoc/en/spot/#trade-channel)

#### Futures Market
- **WebSocket URL**: wss://ws.bitget.com/mix/v1/stream
- **Subscription Format**: identical to spot with futures endpoint
- **Reference**: [Bitget Futures WebSocket Docs](https://bitgetlimited.github.io/apidoc/en/mix/#trade-channel)

### Bybit

#### Spot Market
- **WebSocket URL**: wss://stream.bybit.com/v5/public/spot
- **Subscription Format**:
  ```json
  {
    "op": "subscribe",
    "args": ["publicTrade.<symbol>"]
  }
  ```
- **Reference**: [Bybit WebSocket API Documentation](https://bybit-exchange.github.io/docs/v5/websocket/public/trade)

#### Futures Market
- **WebSocket URL**: wss://stream.bybit.com/v5/public/linear
- **Subscription Format**: same as spot but using futures endpoint
- **Reference**: [Bybit Futures WebSocket Docs](https://bybit-exchange.github.io/docs/v5/websocket/public/trade)

### Coinbase

#### Spot Market
- **WebSocket URL**: wss://ws-feed.exchange.coinbase.com
- **Subscription Format**:
  ```json
  {
    "type": "subscribe",
    "channels": [{"name": "matches", "product_ids": ["<product>"]}]
  }
  ```
- **Reference**: [Coinbase WebSocket API](https://docs.cloud.coinbase.com/exchange/docs/websocket-overview)

#### Futures Market
- **WebSocket URL**: wss://ws-feed.exchange.coinbase.com
- **Subscription Format**: Coinbase uses the same endpoint with futures products when available
- **Reference**: same as spot

### Kraken

#### Spot Market
- **WebSocket URL**: wss://ws.kraken.com/
- **Subscription Format**:
  ```json
  {
    "event": "subscribe",
    "pair": ["<pair>"],
    "subscription": {"name": "trade"}
  }
  ```
- **Reference**: [Kraken WebSocket API](https://docs.kraken.com/websockets/#message-trade)

#### Futures Market
- **WebSocket URL**: wss://futures.kraken.com/ws/v1
- **Subscription Format**:
  ```json
  {
    "event": "subscribe",
    "feed": "trade",
    "product_ids": ["<symbol>"]
  }
  ```
- **Reference**: [Kraken Futures WebSocket Docs](https://docs.futures.kraken.com/#websocket-api-public-feeds-trade)

### Kucoin

#### Spot Market
- **WebSocket URL**: wss://ws-api.kucoin.com/endpoint
- **Subscription Format**:
  ```json
  {
    "id": 1,
    "type": "subscribe",
    "topic": "/market/match:<symbol>",
    "privateChannel": false
  }
  ```
- **Reference**: [Kucoin WebSocket API](https://docs.kucoin.com/#match-execution-data)

#### Futures Market
- **WebSocket URL**: wss://ws-api-futures.kucoin.com/endpoint
- **Subscription Format**: same as spot with futures endpoint
- **Reference**: [Kucoin Futures WebSocket API](https://docs.kucoin.com/futures/#execution-data)

### OKX

#### Spot and Futures Markets
- **WebSocket URL**: wss://ws.okx.com:8443/ws/v5/public
- **Subscription Format**:
  ```json
  {
    "op": "subscribe",
    "args": [{"channel": "trades", "instId": "<instrument>"}]
  }
  ```
- **Reference**: [OKX WebSocket Docs](https://www.okx.com/docs-v5/en/#websocket-api-market-data-public-trades)

### Gate.io

#### Spot Market
- **WebSocket URL**: wss://api.gateio.ws/ws/v4/
- **Subscription Format**:
  ```json
  {
    "time": 0,
    "channel": "spot.trades",
    "event": "subscribe",
    "payload": ["<symbol>"]
  }
  ```
- **Reference**: [Gate.io WebSocket API](https://www.gate.io/docs/developers/apiv4/#spot-trades)

#### Futures Market
- **WebSocket URL**: wss://fx-ws.gateio.ws/v4/ws/
- **Subscription Format**:
  ```json
  {
    "time": 0,
    "channel": "futures.trades",
    "event": "subscribe",
    "payload": ["<settle>", "<contract>"]
  }
  ```
- **Reference**: [Gate.io Futures WebSocket Docs](https://www.gate.io/docs/developers/apiv4/#futures-trades)

### Crypto.com

#### Spot Market
- **WebSocket URL**: wss://stream.crypto.com/v2/market
- **Subscription Format**:
  ```json
  {
    "id": 1,
    "method": "subscribe",
    "params": {"channels": ["trade.<instrument>"]}
  }
  ```
- **Reference**: [Crypto.com WebSocket API](https://exchange-docs.crypto.com/exchange/v1/#trades)

#### Futures Market
- **WebSocket URL**: wss://deriv-stream.crypto.com/v1/market
- **Subscription Format**: same as spot using the derivatives endpoint
- **Reference**: [Crypto.com Derivatives WebSocket API](https://exchange-docs.crypto.com/derivatives/v1/#trades)

### MEXC

#### Spot Market
- **WebSocket URL**: wss://wbs.mexc.com/ws
- **Subscription Format**:
  ```json
  {
    "method": "SUBSCRIPTION",
    "params": ["spot@public.deals.v3.api@<symbol>"]
  }
  ```
- **Reference**: [MEXC Spot WebSocket Docs](https://mexcdevelop.github.io/apidocs/spot_v3_en/#trade-streams)

#### Futures Market
- **WebSocket URL**: wss://contract.mexc.com/ws
- **Subscription Format**:
  ```json
  {
    "method": "sub.deal",
    "param": {"symbol": "<symbol>", "vol": "0"}
  }
  ```
- **Reference**: [MEXC Futures WebSocket Docs](https://mexcdevelop.github.io/apidocs/contract_v1_en/#trade-stream)

### Hyperliquid

#### Spot and Futures Markets
- **WebSocket URL**: wss://api.hyperliquid.xyz/ws
- **Subscription Format**:
  ```json
  {
    "type": "subscribe",
    "channel": "trades",
    "coin": "<asset>"
  }
  ```
- **Reference**: [Hyperliquid WebSocket Docs](https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/websocket/subscriptions)

## Usage Notes

Trade listeners in Jackbot connect to these endpoints using the shown subscription formats. Message structures vary slightly between exchanges but typically include price, amount, side and a timestamp. Consult the official documentation links above for full details and optional parameters.

