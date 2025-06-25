use jackbot_snapshot::*;
use std::{fs, path::PathBuf, sync::Arc, time::Duration};

#[tokio::test]
async fn test_snapshot_once() {
    let redis = Arc::new(FakeRedis::default());
    let store_root = PathBuf::from("local_store");
    let store = Arc::new(LocalStore::new(store_root.clone()));
    let iceberg_metadata = PathBuf::from("iceberg_meta.json");
    let config = SnapshotConfig {
        interval: Duration::from_secs(1),
        retention: Duration::from_secs(3600), // 1 hour
    };
    let scheduler = SnapshotScheduler::new(redis.clone(), store, iceberg_metadata.clone(), config);

    // Insert some data into redis
    redis
        .insert(DataRecord {
            exchange: "test_exchange".to_string(),
            market: "test_market".to_string(),
            record_type: RecordType::OrderBook,
            value: "test_value".to_string(),
        })
        .await;

    // Run snapshot once
    scheduler.snapshot_once().await.unwrap();

    // Check that the file was written to the local store
    // Note: The exact filename is difficult to predict due to the timestamp.
    // We'll check if *a* file was created in the expected directory.
    let mut entries = fs::read_dir(store_root.join("test_exchange/test_market"))
        .unwrap()
        .map(|res| res.map(|e| e.path()))
        .collect::<Result<Vec<_>, std::io::Error>>()
        .unwrap();
    assert_eq!(entries.len(), 1);
    let created_file_path = entries.pop().unwrap();
    assert!(created_file_path.to_string_lossy().contains("snapshot_"));
    assert!(created_file_path.to_string_lossy().ends_with(".parquet"));

    // Check that the iceberg metadata was updated
    let meta_content = fs::read_to_string(&iceberg_metadata).unwrap();
    let meta: IcebergMeta = serde_json::from_str(&meta_content).unwrap();
    assert_eq!(meta.table_location, iceberg_metadata.to_string_lossy());
    assert!(meta.current_snapshot_id > 0); // ID is a timestamp
    assert_eq!(meta.snapshots.len(), 1);
    assert!(meta.snapshots[0].id > 0); // ID is a timestamp
    assert_eq!(meta.snapshots[0].files.len(), 1);
    assert!(
        meta.snapshots[0].files[0]
            .contains(&*created_file_path.file_name().unwrap().to_string_lossy())
    );

    // Cleanup
    let _ = fs::remove_file(created_file_path);
    let _ = fs::remove_file(iceberg_metadata);
    let _ = fs::remove_dir_all(store_root);
}

#[tokio::test]
async fn test_snapshot_skip_empty() {
    let redis = Arc::new(FakeRedis::default());
    let store_root = PathBuf::from("local_store_empty");
    if store_root.exists() {
        fs::remove_dir_all(&store_root).unwrap();
    }
    fs::create_dir_all(&store_root).unwrap();
    let store = Arc::new(LocalStore::new(store_root.clone()));
    let iceberg_metadata = PathBuf::from("iceberg_meta_empty.json");
    if iceberg_metadata.exists() {
        fs::remove_file(&iceberg_metadata).unwrap();
    }
    let config = SnapshotConfig {
        interval: Duration::from_secs(1),
        retention: Duration::from_secs(3600),
    };
    let scheduler = SnapshotScheduler::new(redis.clone(), store, iceberg_metadata.clone(), config);

    // Run snapshot once (redis is empty)
    scheduler.snapshot_once().await.unwrap();

    // Check that no file was written and iceberg metadata is empty
    assert!(fs::read_dir(&store_root).unwrap().next().is_none());
    assert!(!iceberg_metadata.exists());

    // Cleanup
    let _ = fs::remove_dir_all(store_root);
}
