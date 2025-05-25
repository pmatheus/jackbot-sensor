use async_trait::async_trait;
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RecordType {
    OrderBook,
    Trade,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DataRecord {
    pub exchange: String,
    pub market: String,
    pub record_type: RecordType,
    pub value: String,
}

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

#[async_trait]
pub trait StorageBackend: Send + Sync {
    async fn store(&self, local_path: &Path, dest_key: &str) -> io::Result<String>;
    async fn cleanup(&self, retention: Duration) -> io::Result<()>;
}

pub struct LocalStorage {
    root: PathBuf,
}

impl LocalStorage {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

#[async_trait]
impl StorageBackend for LocalStorage {
    async fn store(&self, local_path: &Path, dest_key: &str) -> io::Result<String> {
        let dest = self.root.join(dest_key);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(local_path, &dest)?;
        Ok(dest.display().to_string())
    }

    async fn cleanup(&self, retention: Duration) -> io::Result<()> {
        cleanup_old_files(&self.root, retention)
    }
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
    pub table_location: String,
    pub schema_version: u32,
    pub files: Vec<String>,
}

#[async_trait]
pub trait IcebergCatalog: Send + Sync {
    async fn register(&self, file_path: &str) -> io::Result<()>;
}

pub struct LocalCatalog {
    metadata_path: PathBuf,
}

impl LocalCatalog {
    pub fn new(path: PathBuf) -> Self {
        Self {
            metadata_path: path,
        }
    }
}

#[async_trait]
impl IcebergCatalog for LocalCatalog {
    async fn register(&self, file_path: &str) -> io::Result<()> {
        let mut meta: IcebergMeta = if self.metadata_path.exists() {
            let data = fs::read_to_string(&self.metadata_path)?;
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            IcebergMeta {
                table_location: "local".into(),
                schema_version: 1,
                files: Vec::new(),
            }
        };
        let entry = file_path.to_string();
        if !meta.files.contains(&entry) {
            meta.files.push(entry);
        }
        fs::write(&self.metadata_path, serde_json::to_string(&meta)?)
    }
}

#[derive(Clone)]
pub struct SnapshotConfig {
    pub interval: Duration,
    pub retention: Duration,
}

pub struct SnapshotScheduler {
    redis: Arc<FakeRedis>,
    storage: Arc<dyn StorageBackend>,
    catalog: Arc<dyn IcebergCatalog>,
    config: SnapshotConfig,
}

impl SnapshotScheduler {
    pub fn new(
        redis: Arc<FakeRedis>,
        storage: Arc<dyn StorageBackend>,
        catalog: Arc<dyn IcebergCatalog>,
        config: SnapshotConfig,
    ) -> Self {
        Self {
            redis,
            storage,
            catalog,
            config,
        }
    }

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
        let dest_key = format!("{exchange}/{market}/{file_name}");
        let stored_path = self.storage.store(&local_path, &dest_key).await?;
        self.catalog.register(&stored_path).await?;
        self.storage.cleanup(self.config.retention).await?;
        Ok(())
    }

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
        let cfg = SnapshotConfig {
            interval: Duration::from_millis(1),
            retention: Duration::from_secs(1),
        };
        let storage = Arc::new(LocalStorage::new(s3_root.clone()));
        let catalog = Arc::new(LocalCatalog::new(meta.clone()));
        let scheduler = SnapshotScheduler::new(redis, storage, catalog, cfg);
        scheduler.snapshot_once().await.unwrap();
        assert!(
            fs::read_dir(s3_root.join("exch/btc-usd"))
                .unwrap()
                .next()
                .is_some()
        );
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
        let cfg = SnapshotConfig {
            interval: Duration::from_millis(1),
            retention: Duration::from_secs(1),
        };
        let storage = Arc::new(LocalStorage::new(s3_root.clone()));
        let catalog = Arc::new(LocalCatalog::new(meta.clone()));
        let scheduler = SnapshotScheduler::new(redis, storage, catalog, cfg);
        scheduler.snapshot_once().await.unwrap();
        assert!(!s3_root.exists());
        assert!(!meta.exists());
    }
}
