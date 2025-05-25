# User WebSocket Quirks

This document lists notable quirks and limitations when connecting to private user WebSocket streams across exchanges. Authentication endpoints are summarised in [USER_WS_AUTH.md](USER_WS_AUTH.md).

## Quick Reference

| Exchange | Notes |
|---------|------|
| Binance | HMAC SHA256 signed query login |
| Bitget | HMAC SHA256 with timestamp login |
| Bybit | HMAC SHA256 signature login |
| Coinbase | Spot only; futures trade streams unavailable |
| Kraken | API key + secret challenge |
| Kucoin | JWT token from REST key query |
| OKX | Signature of timestamp and api key |
| Gate.io | Client stub only |
| Crypto.com | Client stub only |
| MEXC | Client stub only |
| Hyperliquid | API key header handshake; perpetual markets only |

## Details

- Coinbase futures trade streams are not currently offered. The futures module remains a stub.
- Hyperliquid offers perpetual futures exclusively.
- Gate.io, Crypto.com and MEXC clients are placeholders awaiting full implementation.
