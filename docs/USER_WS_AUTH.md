# User WebSocket Authentication

This document summarises the authentication schemes and WebSocket endpoints required to access private user data (balances, orders, positions) for each exchange supported by Jackbot.

## Quick Reference

| Exchange | Spot Endpoint | Futures Endpoint | Auth Method |
|---------|---------------|-----------------|-------------|
| Binance | `wss://stream.binance.com:9443/ws` | `wss://fstream.binance.com/ws` | HMAC SHA256 signed query |
| Bitget | `wss://ws.bitget.com/spot/v1/stream` | `wss://ws.bitget.com/mix/v1/stream` | HMAC SHA256 with timestamp |
| Bybit | `wss://stream.bybit.com/v5/private` | `wss://stream.bybit.com/v5/private` | HMAC SHA256 signature |
| Coinbase | `wss://ws-feed.exchange.coinbase.com` | `wss://ws-feed.exchange.coinbase.com` | Signed API key payload |
| Kraken | `wss://ws.kraken.com` | `wss://futures.kraken.com/ws/v1` | API key + secret challenge |
| Kucoin | `wss://ws-api.kucoin.com/endpoint` | `wss://ws-api-futures.kucoin.com/endpoint` | JWT token from REST key query |
| OKX | `wss://ws.okx.com:8443/ws/v5/private` | `wss://ws.okx.com:8443/ws/v5/private` | Signature of timestamp and api key |
| Gate.io | `wss://api.gateio.ws/ws/v4/` | `wss://fx-ws.gateio.ws/v4/ws/` | HMAC SHA512 signed message |
| Crypto.com | `wss://stream.crypto.com/v2/user` | `wss://deriv-stream.crypto.com/v1/private` | HMAC SHA256 with API key |
| MEXC | `wss://wbs.mexc.com/ws` | `wss://contract.mexc.com/ws` | HMAC SHA256 signed login |
| Hyperliquid | `wss://api.hyperliquid.xyz/ws` | `wss://api.hyperliquid.xyz/ws` | API key header handshake |

The exact login payloads and signing procedures are documented in each exchange's official API documentation.
