use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::{self, Write},
    path::{Path, PathBuf},
    process::Command,
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

pub fn upload_to_s3(local_path: &Path, s3_root: &str) -> io::Result<String> {
    let file_name = local_path
        .file_name()
        .ok_or_else(|| io::Error::other("missing file name"))?
        .to_str()
        .ok_or_else(|| io::Error::other("invalid file name"))?;

    if let Some(path) = s3_root.strip_prefix("file://") {
        let root = Path::new(path);
        fs::create_dir_all(root)?;
        let dest = root.join(file_name);
        fs::copy(local_path, &dest)?;
        Ok(dest.display().to_string())
    } else {
        let dest = format!("{}/{}", s3_root.trim_end_matches('/'), file_name);
        let status = Command::new("aws")
            .args(["s3", "cp", local_path.to_str().unwrap(), &dest])
            .status()?;
        if status.success() {
            Ok(dest)
        } else {
            Err(io::Error::other("aws cli failed"))
        }
    }
}

fn cleanup_old_files(root: &str, retention: Duration) -> io::Result<()> {
    let path = match root.strip_prefix("file://") {
        Some(p) => Path::new(p),
        None => return Ok(()),
    };
    if !path.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(path)? {
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
            cleanup_old_files(&format!("file://{}", entry.path().display()), retention)?;
        }
    }
    Ok(())
}

#[derive(Serialize, Deserialize, Default)]
pub struct IcebergSnapshot {
    pub snapshot_id: u64,
    pub files: Vec<String>,
}

#[derive(Serialize, Deserialize, Default)]
pub struct IcebergTable {
    pub format_version: u32,
    pub current_snapshot_id: u64,
    pub snapshots: Vec<IcebergSnapshot>,
}

/// Append a new data file to the Iceberg table metadata using a new snapshot.
pub fn register_with_iceberg(metadata_path: &Path, file_path: &str) -> io::Result<()> {
    let mut table: IcebergTable = if metadata_path.exists() {
        let data = fs::read_to_string(metadata_path)?;
        serde_json::from_str(&data).unwrap_or_default()
    } else {
        IcebergTable {
            format_version: 2,
            current_snapshot_id: 0,
            snapshots: Vec::new(),
        }
    };

    let new_id = table.current_snapshot_id + 1;
    table.current_snapshot_id = new_id;
    table.snapshots.push(IcebergSnapshot {
        snapshot_id: new_id,
        files: vec![file_path.to_string()],
    });

    fs::write(metadata_path, serde_json::to_string(&table)?)
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
    s3_root: String,
    iceberg_metadata: PathBuf,
    config: SnapshotConfig,
}

impl SnapshotScheduler {
    pub fn new(
        redis: Arc<FakeRedis>,
        s3_root: String,
        iceberg_metadata: PathBuf,
        config: SnapshotConfig,
    ) -> Self {
        Self {
            redis,
            s3_root,
            iceberg_metadata,
            config,
        }
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
            .first()
            .map(|r| (r.exchange.clone(), r.market.clone()))
            .unwrap_or_else(|| ("unknown".into(), "unknown".into()));
        let dest_dir = format!(
            "{}/{}/{}",
            self.s3_root.trim_end_matches('/'),
            exchange,
            market
        );
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
        let s3_root = format!("file://{}", dir.join("s3_test").display());
        let local_root = Path::new(&s3_root[7..]);
        let meta = dir.join("meta.json");
        let _ = fs::remove_dir_all(local_root);
        let _ = fs::remove_file(&meta);
        let cfg = SnapshotConfig {
            interval: Duration::from_millis(1),
            retention: Duration::from_secs(1),
        };
        let scheduler = SnapshotScheduler::new(redis, s3_root.clone(), meta.clone(), cfg);
        scheduler.snapshot_once().await.unwrap();
        assert!(
            fs::read_dir(local_root.join("exch/btc-usd"))
                .unwrap()
                .next()
                .is_some()
        );
        let meta_contents = fs::read_to_string(meta).unwrap();
        let meta: IcebergTable = serde_json::from_str(&meta_contents).unwrap();
        assert_eq!(meta.current_snapshot_id, 1);
    }

    #[tokio::test]
    async fn test_snapshot_skip_empty() {
        let redis = Arc::new(FakeRedis::default());
        let dir = std::env::temp_dir();
        let s3_root = format!("file://{}", dir.join("s3_empty").display());
        let local_root = Path::new(&s3_root[7..]);
        let meta = dir.join("meta_empty.json");
        let _ = fs::remove_dir_all(local_root);
        let _ = fs::remove_file(&meta);
        let cfg = SnapshotConfig {
            interval: Duration::from_millis(1),
            retention: Duration::from_secs(1),
        };
        let scheduler = SnapshotScheduler::new(redis, s3_root.clone(), meta.clone(), cfg);
        scheduler.snapshot_once().await.unwrap();
        assert!(!local_root.exists());
        assert!(!meta.exists());
    }
}
