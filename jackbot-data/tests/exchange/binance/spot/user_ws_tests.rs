use jackbot_data::exchange::binance::spot::user_ws::{user_stream, BinanceUserEvent};
use tokio::net::TcpListener;
use tokio_stream::StreamExt;
use tokio_tungstenite::{accept_async, tungstenite::Message};
use url::Url;

async fn run_server(addr: &str, first: String, second: String) {
    let listener = TcpListener::bind(addr).await.unwrap();
    for payload in [first, second] {
        let (stream, _) = listener.accept().await.unwrap();
        let mut ws = accept_async(stream).await.unwrap();
        // recv auth
        ws.next().await.unwrap().unwrap();
        ws.send(Message::Text(payload.into())).await.unwrap();
        ws.close(None).await.unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_user_stream_parse() {
    let addr = "127.0.0.1:18091"; // Changed port to avoid conflict with other tests
    let first =
        r#"{"e":"balance","E":1,"asset":"BTC","free":"0.5","total":"1.0"}"#
            .to_string();
    let second = r#"{"e":"order","E":2,"s":"BTCUSDT","S":"BUY","p":"100","q":"0.1","i":1,"X":"NEW"}"#.to_string();
    tokio::spawn(run_server(addr, first.clone(), second.clone()));

    let mut stream = user_stream(
        Url::parse(&format!("ws://{}", addr)).unwrap(),
        "{}".to_string(),
    )
    .await
    .unwrap();
    
    // Add timeout to prevent test hanging
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        async {
            let ev1 = stream.next().await.unwrap();
            assert!(matches!(ev1, BinanceUserEvent::Balance { .. }));
            let ev2 = stream.next().await.unwrap();
            assert!(matches!(ev2, BinanceUserEvent::Order { .. }));
        }
    ).await;
    
    assert!(result.is_ok(), "Test timed out after 5 seconds");
}
