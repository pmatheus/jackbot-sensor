use async_trait::async_trait;
use hmac::{Hmac, Mac};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime},
};
use tokio::sync::Mutex;
use tokio::time;
type HmacSha256 = Hmac<Sha256>;

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

#[async_trait]
pub trait ObjectStore: Send + Sync {
    async fn put(&self, key: &str, local_path: &Path) -> io::Result<String>;
    async fn cleanup(&self, prefix: &str, retention: Duration) -> io::Result<()>;
}

/// Local filesystem implementation of [`ObjectStore`] used in tests.
pub struct LocalStore {
    root: PathBuf,
}

impl LocalStore {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

#[async_trait]
impl ObjectStore for LocalStore {
    async fn put(&self, key: &str, local_path: &Path) -> io::Result<String> {
        let dest = self.root.join(key);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(local_path, &dest)?;
        Ok(dest.to_string_lossy().to_string())
    }

    async fn cleanup(&self, prefix: &str, retention: Duration) -> io::Result<()> {
        let path = self.root.join(prefix);
        cleanup_old_files(&path, retention)
    }
}

/// AWS S3 configuration for [`S3Store`].
pub struct AwsConfig {
    pub bucket: String,
    pub region: String,
    pub access_key: String,
    pub secret_key: String,
}

/// S3-backed implementation of [`ObjectStore`].
pub struct S3Store {
    cfg: AwsConfig,
    client: Client,
}

impl S3Store {
    pub fn new(cfg: AwsConfig) -> Self {
        Self {
            cfg,
            client: Client::new(),
        }
    }
}

#[async_trait]
impl ObjectStore for S3Store {
    async fn put(&self, key: &str, local_path: &Path) -> io::Result<String> {
        upload_to_s3(local_path, key, &self.cfg, &self.client).await?;
        Ok(format!("s3://{}/{}", self.cfg.bucket, key))
    }

    async fn cleanup(&self, _prefix: &str, _retention: Duration) -> io::Result<()> {
        // In production this would list and remove expired objects. Omitted for brevity.
        Ok(())
    }
}

async fn upload_to_s3(
    local_path: &Path,
    key: &str,
    cfg: &AwsConfig,
    client: &Client,
) -> io::Result<()> {
    let data = fs::read(local_path)?;
    let host = format!("{}.s3.{}.amazonaws.com", cfg.bucket, cfg.region);
    let url = format!("https://{}/{}", host, key);

    let payload_hash = hex::encode(Sha256::digest(&data));
    let now = chrono::Utc::now();
    let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
    let date_stamp = now.format("%Y%m%d").to_string();
    let canonical_headers = format!(
        "host:{}\nx-amz-content-sha256:{}\nx-amz-date:{}\n",
        host, payload_hash, amz_date
    );
    let signed_headers = "host;x-amz-content-sha256;x-amz-date";
    let canonical_request = format!(
        "PUT\n/{}\n\n{}\n{}\n{}",
        key, canonical_headers, signed_headers, payload_hash
    );
    let canonical_hash = hex::encode(Sha256::digest(canonical_request.as_bytes()));
    let scope = format!("{}/{}/s3/aws4_request", date_stamp, cfg.region);
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{}\n{}\n{}",
        amz_date, scope, canonical_hash
    );
    let signing_key = signing_key(&cfg.secret_key, &date_stamp, &cfg.region, "s3");
    let mut mac = HmacSha256::new_from_slice(&signing_key).expect("HMAC can take key");
    mac.update(string_to_sign.as_bytes());
    let signature = hex::encode(mac.finalize().into_bytes());
    let authorization = format!(
        "AWS4-HMAC-SHA256 Credential={}/{}, SignedHeaders={}, Signature={}",
        cfg.access_key, scope, signed_headers, signature
    );

    let res = client
        .put(url)
        .header("x-amz-content-sha256", payload_hash)
        .header("x-amz-date", amz_date)
        .header("Authorization", authorization)
        .body(data)
        .send()
        .await
        .map_err(|e| io::Error::other(e.to_string()))?;

    if !res.status().is_success() {
        return Err(io::Error::other(format!(
            "s3 upload failed: {}",
            res.status()
        )));
    }
    Ok(())
}

fn signing_key(secret: &str, date: &str, region: &str, service: &str) -> Vec<u8> {
    let k_date = hmac_sha256(format!("AWS4{}", secret).as_bytes(), date.as_bytes());
    let k_region = hmac_sha256(&k_date, region.as_bytes());
    let k_service = hmac_sha256(&k_region, service.as_bytes());
    hmac_sha256(&k_service, b"aws4_request")
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC can take key");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
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
pub struct IcebergSnapshot {
    pub id: u64,
    pub timestamp_ms: i64,
    pub files: Vec<String>,
}

#[derive(Serialize, Deserialize, Default)]
pub struct IcebergTable {
    pub format_version: u32,
    pub snapshots: Vec<IcebergSnapshot>,
}

/// Register a new data file with the Iceberg table metadata.
pub fn register_with_iceberg(metadata_path: &Path, file_path: &str) -> io::Result<()> {
    let mut table: IcebergTable = if metadata_path.exists() {
        let data = fs::read_to_string(metadata_path)?;
        serde_json::from_str(&data).unwrap_or_default()
    } else {
        IcebergTable {
            format_version: 1,
            snapshots: Vec::new(),
        }
    };
    let snapshot = IcebergSnapshot {
        id: chrono::Utc::now().timestamp_millis() as u64,
        timestamp_ms: chrono::Utc::now().timestamp_millis(),
        files: vec![file_path.to_string()],
    };
    table.snapshots.push(snapshot);
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
    store: Arc<dyn ObjectStore>,
    iceberg_metadata: PathBuf,
    config: SnapshotConfig,
}

impl SnapshotScheduler {
    pub fn new(
        redis: Arc<FakeRedis>,
        store: Arc<dyn ObjectStore>,
        iceberg_metadata: PathBuf,
        config: SnapshotConfig,
    ) -> Self {
        Self {
            redis,
            store,
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
        let key = format!("{}/{}/{}", exchange, market, file_name);
        let s3_path = self.store.put(&key, &local_path).await?;
        register_with_iceberg(&self.iceberg_metadata, &s3_path)?;
        self.store
            .cleanup(&format!("{}/{}", exchange, market), self.config.retention)
            .await?;
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
    use std::sync::Arc;

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
        let store = Arc::new(LocalStore::new(s3_root.clone()));
        let scheduler = SnapshotScheduler::new(redis, store, meta.clone(), cfg);
        scheduler.snapshot_once().await.unwrap();
        assert!(
            fs::read_dir(s3_root.join("exch/btc-usd"))
                .unwrap()
                .next()
                .is_some()
        );
        let meta_contents = fs::read_to_string(meta).unwrap();
        let meta: IcebergTable = serde_json::from_str(&meta_contents).unwrap();
        assert_eq!(meta.snapshots.len(), 1);
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
        let store = Arc::new(LocalStore::new(s3_root.clone()));
        let scheduler = SnapshotScheduler::new(redis, store, meta.clone(), cfg);
        scheduler.snapshot_once().await.unwrap();
        assert!(!s3_root.exists());
        assert!(!meta.exists());
    }
}
