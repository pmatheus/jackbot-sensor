#!/usr/bin/env python3
"""
EXCHANGE CONNECTION DESTRUCTION TEST
Tests real exchange WebSocket connectivity claims
"""

import asyncio
import time
import websockets
import json
from datetime import datetime

async def test_exchange_connection(name, url, subscribe_msg):
    """Test connection to a single exchange"""
    print(f"\nTesting {name}: {url}")
    
    try:
        # Connection timing
        start_time = time.time()
        async with websockets.connect(url, timeout=10) as websocket:
            connect_time = (time.time() - start_time) * 1000
            print(f"  ✅ Connected in {connect_time:.2f}ms")
            
            # Send subscription
            if subscribe_msg:
                await websocket.send(json.dumps(subscribe_msg))
                
                # Measure first message latency
                msg_start = time.time()
                response = await asyncio.wait_for(websocket.recv(), timeout=5)
                msg_latency = (time.time() - msg_start) * 1000
                
                print(f"  📨 First message in {msg_latency:.2f}ms")
                
                if msg_latency < 10:
                    print(f"  ✅ UNDER 10ms latency!")
                else:
                    print(f"  ❌ OVER 10ms - Claim questionable!")
                    
                # Show message preview
                if len(response) > 100:
                    print(f"  Message: {response[:100]}...")
                else:
                    print(f"  Message: {response}")
                    
            return True, connect_time, msg_latency if subscribe_msg else 0
            
    except asyncio.TimeoutError:
        print(f"  ❌ Connection timeout!")
        return False, 0, 0
    except Exception as e:
        print(f"  ❌ Error: {e}")
        return False, 0, 0

async def main():
    print("=== EXCHANGE CONNECTION DESTRUCTION TEST ===")
    print(f"Test started at: {datetime.now()}")
    
    # Exchange configurations
    exchanges = [
        {
            "name": "Binance",
            "url": "wss://stream.binance.com:9443/ws",
            "subscribe": {
                "method": "SUBSCRIBE",
                "params": ["btcusdt@ticker"],
                "id": 1
            }
        },
        {
            "name": "Coinbase",
            "url": "wss://ws-feed.exchange.coinbase.com",
            "subscribe": {
                "type": "subscribe",
                "channels": [{"name": "ticker", "product_ids": ["BTC-USD"]}]
            }
        },
        {
            "name": "Bybit",
            "url": "wss://stream.bybit.com/v5/public/spot",
            "subscribe": {
                "op": "subscribe",
                "args": ["tickers.BTCUSDT"]
            }
        },
        {
            "name": "Kraken",
            "url": "wss://ws.kraken.com",
            "subscribe": {
                "event": "subscribe",
                "pair": ["XBT/USD"],
                "subscription": {"name": "ticker"}
            }
        },
        {
            "name": "OKX",
            "url": "wss://ws.okx.com:8443/ws/v5/public",
            "subscribe": {
                "op": "subscribe",
                "args": [{"channel": "tickers", "instId": "BTC-USDT"}]
            }
        }
    ]
    
    # Test each exchange
    results = []
    for exchange in exchanges:
        success, conn_time, msg_latency = await test_exchange_connection(
            exchange["name"], 
            exchange["url"], 
            exchange.get("subscribe")
        )
        results.append({
            "name": exchange["name"],
            "success": success,
            "connection_time": conn_time,
            "message_latency": msg_latency
        })
    
    # Summary report
    print("\n=== CONNECTION DESTRUCTION REPORT ===")
    successful = sum(1 for r in results if r["success"])
    print(f"Connected: {successful}/{len(results)}")
    
    print("\nDetailed Results:")
    for result in results:
        if result["success"]:
            print(f"{result['name']}: ✅ Connected in {result['connection_time']:.2f}ms, "
                  f"message latency: {result['message_latency']:.2f}ms")
        else:
            print(f"{result['name']}: ❌ FAILED")
    
    # Latency claim verification
    print("\n=== <10ms LATENCY CLAIM VERIFICATION ===")
    under_10ms = sum(1 for r in results if r["success"] and r["message_latency"] < 10)
    print(f"Exchanges with <10ms latency: {under_10ms}/{successful}")
    
    if under_10ms == 0:
        print("🚨 CLAIM DESTROYED: NO exchange achieved <10ms latency!")
    elif under_10ms < successful:
        print("⚠️  CLAIM PARTIALLY FALSE: Only some exchanges achieve <10ms")
    else:
        print("✅ CLAIM VERIFIED: All exchanges achieve <10ms")

if __name__ == "__main__":
    asyncio.run(main())