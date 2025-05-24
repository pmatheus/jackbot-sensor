# Redis Snapshot Pipeline

This document describes how Jackbot persists order book and trade data from Redis to an S3-backed data lake managed with Apache Iceberg.

## Workflow

1. **Collect Data**: Exchange modules store normalized `DataRecord` items in Redis. Each record contains the exchange, market, record type, and a serialized value.
2. **Snapshot Scheduler**: `SnapshotScheduler` periodically fetches all records from Redis. If no data is present, the scheduler skips creating a snapshot.
3. **Parquet Serialization**: Records are serialized to a temporary Parquet file.
4. **Upload to S3**: The file is uploaded to `s3://<root>/<exchange>/<market>/` using a unique timestamped name. Older files are removed based on the configured retention period.
5. **Iceberg Registration**: The S3 path is appended to a simple Iceberg metadata file. Duplicate paths are ignored.

## Configuration

The scheduler interval and retention are specified via `SnapshotConfig`. Running `SnapshotScheduler::start` will continuously persist snapshots according to this configuration.

This pipeline allows efficient, queryable storage of historical market data while keeping Redis memory usage under control.
