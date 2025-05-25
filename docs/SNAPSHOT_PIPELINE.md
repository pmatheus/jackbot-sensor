# Redis Snapshot Pipeline

This document describes how Jackbot persists order book and trade data from Redis to an S3-backed data lake managed with Apache Iceberg.

## Workflow

1. **Collect Data**: Exchange modules store normalized `DataRecord` items in Redis. Each record contains the exchange, market, record type, and a serialized value.
2. **Snapshot Scheduler**: `SnapshotScheduler` periodically fetches all records from Redis. If no data is present, the scheduler skips creating a snapshot.
3. **Parquet Serialization**: Records are serialized to a temporary Parquet file.
4. **Upload to S3**: The file is uploaded to `s3://<bucket>/<exchange>/<market>/` using AWS credentials from the environment. A `file://` prefix may be used to store snapshots locally during testing. Older files are removed based on the configured retention period for local paths.
5. **Iceberg Registration**: The uploaded file is recorded as a new snapshot in the Iceberg table metadata, incrementing the `current_snapshot_id`.


## Configuration

The scheduler interval and retention are specified via `SnapshotConfig`. Running `SnapshotScheduler::start` will continuously persist snapshots according to this configuration.

This pipeline allows efficient, queryable storage of historical market data while keeping Redis memory usage under control.

## Limitations

SnapshotScheduler currently shells out to the `aws` CLI for uploads instead of using the internal `S3Store`. The CLI must be installed and available in PATH. Cleanup of expired objects in S3 is not implemented, so only local paths benefit from retention pruning. Each snapshot writes all Redis records and may grow large on busy systems.
