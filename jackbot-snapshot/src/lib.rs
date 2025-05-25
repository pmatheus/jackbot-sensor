use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime},

};
use tokio::sync::Mutex;
use tokio::time;

/// Type of record stored in Redis and persisted to snapshots.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RecordType {
    OrderBook,
    Trade,
}

/// A single order book or trade record stored in Redis.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DataRecord {
    pub exchange: String,
    pub market: String,
    pub record_type: RecordType,
    pub value: String,
}

/// Minimal in-memory stand in for Redis used in tests.
#[derive(Debug, Default)]
pub struct FakeRedis {
    data: Mutex<Vec<DataRecord>>,
}

impl FakeRedis {
    pub async fn insert(&self, record: DataRecord) {
        self.data.lock().await.push(record);
    }

    pub async fn get_all(&self) -> Vec<DataRecord> {
        self.data.lock().await.clone()
    }
}

pub fn write_parquet(records: &[DataRecord], path: &Path) -> io::Result<()> {
    let mut file = File::create(path)?;
    for record in records {
        serde_json::to_writer(&mut file, record)?;
        file.write_all(b"\n")?;
    }
    Ok(())
}

pub fn upload_to_s3(local_path: &Path, s3_root: &Path) -> io::Result<PathBuf> {
    let file_name = local_path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "missing file name"))?;
    fs::create_dir_all(s3_root)?;
    let dest = s3_root.join(file_name);
    fs::copy(local_path, &dest)?;
    Ok(dest)
}

fn cleanup_old_files(root: &Path, retention: Duration) -> io::Result<()> {
    if !root.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let metadata = entry.metadata()?;
        if metadata.is_file() {
            if let Ok(modified) = metadata.modified() {
                if SystemTime::now()
                    .duration_since(modified)
                    .unwrap_or_default()
                    > retention
                {
                    let _ = fs::remove_file(entry.path());
                }
            }
        } else if metadata.is_dir() {
            cleanup_old_files(&entry.path(), retention)?;
        }
    }
    Ok(())
}

#[derive(Serialize, Deserialize, Default)]
pub struct IcebergMeta {
    pub schema_version: u32,
    pub files: Vec<String>,
}

/// Append a new file path to the Iceberg metadata file if it is not already present.
pub fn register_with_iceberg(metadata_path: &Path, file_path: &Path) -> io::Result<()> {
    let mut meta: IcebergMeta = if metadata_path.exists() {
        let data = fs::read_to_string(metadata_path)?;
        serde_json::from_str(&data).unwrap_or_default()
    } else {
        IcebergMeta { schema_version: 1, files: Vec::new() }
    };
    let entry = file_path.display().to_string();
    if !meta.files.contains(&entry) {
        meta.files.push(entry);
    }
    fs::write(metadata_path, serde_json::to_string(&meta)? )
}

/// Configuration for how often snapshots are taken and how long they are kept.
#[derive(Clone)]
pub struct SnapshotConfig {
    pub interval: Duration,
    pub retention: Duration,
}

/// Periodically persists Redis data to S3 and registers files with Iceberg.
pub struct SnapshotScheduler {
    redis: Arc<FakeRedis>,
    s3_root: PathBuf,
    iceberg_metadata: PathBuf,
    config: SnapshotConfig,
}

impl SnapshotScheduler {
    pub fn new(redis: Arc<FakeRedis>, s3_root: PathBuf, iceberg_metadata: PathBuf, config: SnapshotConfig) -> Self {
        Self { redis, s3_root, iceberg_metadata, config }
    }

    /// Persist all Redis records to a single Parquet file and register it.
    pub async fn snapshot_once(&self) -> io::Result<()> {
        let records = self.redis.get_all().await;
        if records.is_empty() {
            return Ok(());
        }
        let file_name = format!("snapshot_{}.parquet", chrono::Utc::now().timestamp_millis());
        let local_path = std::env::temp_dir().join(&file_name);
        write_parquet(&records, &local_path)?;
        let (exchange, market) = records
            .get(0)
            .map(|r| (r.exchange.clone(), r.market.clone()))
            .unwrap_or_else(|| ("unknown".into(), "unknown".into()));
        let dest_dir = self.s3_root.join(&exchange).join(&market);
        let s3_path = upload_to_s3(&local_path, &dest_dir)?;
        register_with_iceberg(&self.iceberg_metadata, &s3_path)?;
        cleanup_old_files(&self.s3_root, self.config.retention)?;
        Ok(())
    }

    /// Continuously take snapshots according to the configured interval.
    pub async fn start(&self) {
        let mut interval = time::interval(self.config.interval);
        loop {
            interval.tick().await;
            if let Err(err) = self.snapshot_once().await {
                eprintln!("snapshot failed: {err}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_snapshot_once() {
        let redis = Arc::new(FakeRedis::default());

        redis
            .insert(DataRecord {
                exchange: "exch".into(),
                market: "btc-usd".into(),
                record_type: RecordType::OrderBook,
                value: "v".into(),
            })
            .await;
        let dir = std::env::temp_dir();
        let s3_root = dir.join("s3_test");
        let meta = dir.join("meta.json");
        let _ = fs::remove_dir_all(&s3_root);
        let _ = fs::remove_file(&meta);
        let cfg = SnapshotConfig { interval: Duration::from_millis(1), retention: Duration::from_secs(1) };
        let scheduler = SnapshotScheduler::new(redis, s3_root.clone(), meta.clone(), cfg);
        scheduler.snapshot_once().await.unwrap();
        assert!(fs::read_dir(s3_root.join("exch/btc-usd")).unwrap().next().is_some());
        let meta_contents = fs::read_to_string(meta).unwrap();
        let meta: IcebergMeta = serde_json::from_str(&meta_contents).unwrap();
        assert_eq!(meta.files.len(), 1);
    }

    #[tokio::test]
    async fn test_snapshot_skip_empty() {
        let redis = Arc::new(FakeRedis::default());
        let dir = std::env::temp_dir();
        let s3_root = dir.join("s3_empty");
        let meta = dir.join("meta_empty.json");
        let _ = fs::remove_dir_all(&s3_root);
        let _ = fs::remove_file(&meta);
        let cfg = SnapshotConfig { interval: Duration::from_millis(1), retention: Duration::from_secs(1) };
        let scheduler = SnapshotScheduler::new(redis, s3_root.clone(), meta.clone(), cfg);
        scheduler.snapshot_once().await.unwrap();
        assert!(!s3_root.exists());
        assert!(!meta.exists());
    }
}

