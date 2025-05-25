use jackbot_snapshot::{
    DataRecord, FakeRedis, IcebergMeta, RecordType, SnapshotConfig, SnapshotScheduler,
};
use std::{path::PathBuf, sync::Arc, time::Duration};

#[tokio::test]
async fn test_scheduler_multiple_snapshots() {
    let redis = Arc::new(FakeRedis::default());
    redis
        .insert(DataRecord {
            exchange: "exch".into(),
            market: "eth-usd".into(),
            record_type: RecordType::Trade,
            value: "v1".into(),
        })
        .await;
    let dir = std::env::temp_dir();
    let s3_root = dir.join("s3_integration");
    let meta = dir.join("meta_integration.json");
    let _ = std::fs::remove_dir_all(&s3_root);
    let _ = std::fs::remove_file(&meta);
    let cfg = SnapshotConfig {
        interval: Duration::from_millis(1),
        retention: Duration::from_secs(1),
    };
    let scheduler = SnapshotScheduler::new(
        redis,
        s3_root.to_string_lossy().to_string(),
        meta.clone(),
        cfg,
    );
    // Take two snapshots manually
    scheduler.snapshot_once().await.unwrap();
    tokio::time::sleep(Duration::from_millis(1)).await;
    scheduler.snapshot_once().await.unwrap();

    let files: Vec<_> = std::fs::read_dir(PathBuf::from(&s3_root).join("exch/eth-usd"))
        .unwrap()
        .collect();
    assert_eq!(files.len(), 2);
    let meta_contents = std::fs::read_to_string(meta).unwrap();
    let meta: IcebergMeta = serde_json::from_str(&meta_contents).unwrap();
    assert_eq!(meta.snapshots.len(), 2);
}
