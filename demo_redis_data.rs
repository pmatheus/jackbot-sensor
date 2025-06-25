use chrono::Utc;
use rust_decimal_macros::dec;
use jackbot_data::{
    redis_store::{InMemoryStore, RedisStore},
    books::OrderBook,
    subscription::{
        book::OrderBookEvent,
        trade::PublicTrade,
    },
};
use jackbot_instrument::{
    exchange::ExchangeId,
    Side,
};

fn main() {
    println!("🚀 JackBot Market Data Storage Demo");
    println!("===================================\n");
    
    // Create an in-memory Redis store
    let store = InMemoryStore::new();
    
    // Demo 1: Store L2 OrderBook Snapshot
    println!("📊 Storing L2 OrderBook Snapshot for BTC/USDT...");
    let snapshot = OrderBook::new(
        Utc::now().timestamp_millis() as u64,
        Some(12345), // sequence number
        vec![
            (dec!(99500.50), dec!(1.25)),  // bid: price, quantity
            (dec!(99450.00), dec!(2.10)),
            (dec!(99400.75), dec!(0.85)),
        ],
        vec![
            (dec!(99550.25), dec!(0.95)),  // ask: price, quantity  
            (dec!(99600.00), dec!(1.75)),
            (dec!(99650.50), dec!(3.20)),
        ],
    );
    
    store.store_snapshot(ExchangeId::BinanceSpot, "BTCUSDT", &snapshot);
    println!("✅ Snapshot stored successfully!");
    
    // Demo 2: Store OrderBook Delta (Update)
    println!("\n📈 Storing OrderBook Delta Update...");
    let delta_book = OrderBook::new(
        Utc::now().timestamp_millis() as u64,
        Some(12346),
        vec![
            (dec!(99525.00), dec!(1.50)),  // updated bid
        ],
        vec![
            (dec!(99575.00), dec!(0.75)),  // updated ask
        ],
    );
    
    let delta = OrderBookEvent::Update(delta_book);
    store.store_delta(ExchangeId::BinanceSpot, "BTCUSDT", &delta);
    println!("✅ Delta stored successfully!");
    
    // Demo 3: Store Trade Data
    println!("\n💰 Storing Trade Data...");
    let trades = vec![
        PublicTrade {
            id: "trade_001".to_string(),
            price: 99525.50,
            amount: 0.125,
            side: Side::Buy,
        },
        PublicTrade {
            id: "trade_002".to_string(), 
            price: 99530.25,
            amount: 0.250,
            side: Side::Sell,
        },
        PublicTrade {
            id: "trade_003".to_string(),
            price: 99535.00,
            amount: 0.075,
            side: Side::Buy,
        },
    ];
    
    for trade in &trades {
        store.store_trade(ExchangeId::BinanceSpot, "BTCUSDT", trade);
        println!("  📝 Trade {}: {} {} @ ${}", 
                trade.id, 
                if trade.side == Side::Buy { "BUY" } else { "SELL" },
                trade.amount,
                trade.price
        );
    }
    
    // Demo 4: Retrieve and Display Stored Data
    println!("\n🔍 Retrieving Stored Data:");
    println!("=========================");
    
    // Get snapshot
    if let Some(retrieved_snapshot) = store.get_snapshot(ExchangeId::BinanceSpot, "BTCUSDT") {
        println!("\n📊 Latest OrderBook Snapshot:");
        println!("  Sequence: {:?}", retrieved_snapshot.sequence);
        println!("  Timestamp: {}", retrieved_snapshot.time);
        println!("  Bids (price @ quantity):");
        for (price, qty) in &retrieved_snapshot.bids {
            println!("    ${} @ {}", price, qty);
        }
        println!("  Asks (price @ quantity):");
        for (price, qty) in &retrieved_snapshot.asks {
            println!("    ${} @ {}", price, qty);
        }
    }
    
    // Get deltas
    let deltas = store.get_deltas(ExchangeId::BinanceSpot, "BTCUSDT", 5);
    println!("\n📈 Recent OrderBook Deltas: {} updates", deltas.len());
    for (i, delta) in deltas.iter().enumerate() {
        match delta {
            OrderBookEvent::Update(book) => {
                println!("  Delta {}: {} bids, {} asks", 
                        i + 1, 
                        book.bids.len(), 
                        book.asks.len()
                );
            }
            OrderBookEvent::Snapshot(book) => {
                println!("  Snapshot {}: {} bids, {} asks", 
                        i + 1, 
                        book.bids.len(), 
                        book.asks.len()
                );
            }
        }
    }
    
    // Get trades
    let retrieved_trades = store.get_trades(ExchangeId::BinanceSpot, "BTCUSDT", 10);
    println!("\n💰 Recent Trades: {} trades", retrieved_trades.len());
    for (i, trade) in retrieved_trades.iter().enumerate() {
        println!("  Trade {}: {} {} @ ${} (ID: {})", 
                i + 1,
                if trade.side == Side::Buy { "BUY" } else { "SELL" },
                trade.amount,
                trade.price,
                trade.id
        );
    }
    
    // Demo 5: Multiple Exchange Data
    println!("\n🌐 Storing Data for Multiple Exchanges...");
    
    // Store ETH data for OKX
    let eth_trade = PublicTrade {
        id: "okx_trade_001".to_string(),
        price: 3850.75,
        amount: 2.5,
        side: Side::Buy,
    };
    store.store_trade(ExchangeId::Okx, "ETHUSDT", &eth_trade);
    
    // Store SOL data for Bybit
    let sol_snapshot = OrderBook::new(
        Utc::now().timestamp_millis() as u64,
        Some(54321),
        vec![(dec!(245.50), dec!(10.0))],
        vec![(dec!(246.00), dec!(8.5))],
    );
    store.store_snapshot(ExchangeId::Bybit, "SOLUSDT", &sol_snapshot);
    
    println!("✅ Multi-exchange data stored!");
    
    // Summary
    println!("\n📋 Storage Summary:");
    println!("==================");
    println!("• BTC/USDT (Binance): 1 snapshot, {} deltas, {} trades", 
            store.get_deltas(ExchangeId::BinanceSpot, "BTCUSDT", 100).len(),
            store.get_trades(ExchangeId::BinanceSpot, "BTCUSDT", 100).len()
    );
    println!("• ETH/USDT (OKX): {} trades", 
            store.get_trades(ExchangeId::Okx, "ETHUSDT", 100).len()
    );
    println!("• SOL/USDT (Bybit): 1 snapshot");
    
    println!("\n🎉 Demo completed! This shows how JackBot stores and retrieves:");
    println!("   📊 L2 OrderBook snapshots (full market depth)");
    println!("   📈 OrderBook deltas (incremental updates)");
    println!("   💰 Trade data (executed transactions)");
    println!("   🌐 Multi-exchange support");
    println!("\n💡 In production, this data flows from WebSocket streams to Redis!");
}