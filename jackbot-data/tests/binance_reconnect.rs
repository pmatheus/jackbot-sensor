use jackbot_instrument::exchange::ExchangeId;
use jackbot_integration::protocol::websocket::{WsError, WsMessage, with_heartbeat};
use std::io;
use std::time::Duration;

#[tokio::test]
async fn test_heartbeat_timeout_results_in_error() {
    // Uncomment this if tokio::time::pause is available in your version
    // tokio::time::pause();

    let stream = with_heartbeat(
        tokio_stream::pending::<Result<WsMessage, WsError>>(),
        Duration::from_secs(1),
        ExchangeId::BinanceSpot,
    );

    tokio::pin!(stream);

    // Use sleep instead of advance if advance is not available
    tokio::time::sleep(Duration::from_secs(2)).await;
    // tokio::time::advance(Duration::from_secs(2)).await;

    match tokio_stream::StreamExt::next(&mut stream).await {
        Some(Err(WsError::Io(err))) => {
            assert_eq!(err.kind(), io::ErrorKind::TimedOut);
        }
        other => panic!("unexpected result: {:?}", other),
    }
}
