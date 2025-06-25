use super::ConsumerConfig;
use aws_sdk_kinesis::{types::Record, Client as KinesisClient};
use std::{collections::HashMap, time::Duration};
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

/// Kinesis message consumer
#[derive(Debug)]
pub struct KinesisMessageConsumer {
    client: KinesisClient,
    config: ConsumerConfig,
    shard_iterators: HashMap<String, Option<String>>,
}

impl KinesisMessageConsumer {
    /// Create new Kinesis consumer
    pub fn new(
        client: KinesisClient,
        config: ConsumerConfig,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Ok(Self {
            client,
            config,
            shard_iterators: HashMap::new(),
        })
    }

    /// Consume records from a specific stream
    pub async fn consume_records(
        &mut self,
        stream_name: &str,
        max_records: u32,
    ) -> Result<Vec<Record>, Box<dyn std::error::Error + Send + Sync>> {
        // Get shard iterator if we don't have one
        if !self.shard_iterators.contains_key(stream_name) {
            self.initialize_shard_iterator(stream_name).await?;
        }

        let shard_iterator = match self
            .shard_iterators
            .get(stream_name)
            .and_then(|s| s.as_ref())
        {
            Some(iterator) => iterator.clone(),
            None => {
                debug!("No shard iterator available for stream: {}", stream_name);
                return Ok(Vec::new());
            }
        };

        // Get records with retry logic
        let mut attempts = 0;
        let max_attempts = self.config.retry_config.max_attempts;

        while attempts < max_attempts {
            let get_records_result = self
                .get_records_with_iterator(&shard_iterator, max_records)
                .await;
            match get_records_result {
                Ok((records, next_iterator)) => {
                    // Update shard iterator for next poll
                    self.shard_iterators
                        .insert(stream_name.to_string(), next_iterator);
                    return Ok(records);
                }
                Err(e) => {
                    attempts += 1;
                    if attempts >= max_attempts {
                        error!(
                            "Failed to get records from stream {} after {} attempts: {}",
                            stream_name, max_attempts, e
                        );
                        return Err(e);
                    }

                    let delay = self.calculate_retry_delay(attempts);
                    warn!(
                        "Retrying get records for stream {} (attempt {}/{}), waiting {}ms",
                        stream_name, attempts, max_attempts, delay
                    );
                    sleep(Duration::from_millis(delay)).await;
                }
            }
        }

        Ok(Vec::new())
    }

    /// Initialize shard iterator for a stream
    async fn initialize_shard_iterator(
        &mut self,
        stream_name: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        debug!("Initializing shard iterator for stream: {}", stream_name);

        // First, describe the stream to get shard information
        let describe_response = self
            .client
            .describe_stream()
            .stream_name(stream_name)
            .send()
            .await?;

        let stream_description = describe_response
            .stream_description()
            .ok_or("No stream description found")?;

        // Get the first shard (simplified - in production, you'd handle multiple shards)
        let shard = stream_description
            .shards()
            .first()
            .ok_or("No shards found in stream")?;

        let shard_id = shard.shard_id();

        // Get shard iterator
        let shard_iterator_response = self
            .client
            .get_shard_iterator()
            .stream_name(stream_name)
            .shard_id(shard_id)
            .shard_iterator_type(aws_sdk_kinesis::types::ShardIteratorType::from(
                self.config.shard_iterator_type.as_str(),
            ))
            .send()
            .await?;

        let shard_iterator = shard_iterator_response
            .shard_iterator()
            .map(|s| s.to_string());

        self.shard_iterators
            .insert(stream_name.to_string(), shard_iterator);

        info!("Initialized shard iterator for stream: {}", stream_name);
        Ok(())
    }

    /// Get records using shard iterator
    async fn get_records_with_iterator(
        &self,
        shard_iterator: &str,
        max_records: u32,
    ) -> Result<(Vec<Record>, Option<String>), Box<dyn std::error::Error + Send + Sync>> {
        let response = self
            .client
            .get_records()
            .shard_iterator(shard_iterator)
            .set_limit(Some(max_records as i32))
            .send()
            .await?;

        let records = response.records().to_vec();
        let next_shard_iterator = response.next_shard_iterator().map(|s| s.to_string());

        Ok((records, next_shard_iterator))
    }

    /// Calculate exponential backoff retry delay
    fn calculate_retry_delay(&self, attempt: u32) -> u64 {
        let delay = self.config.retry_config.base_delay_ms as f64
            * self
                .config
                .retry_config
                .backoff_multiplier
                .powi(attempt as i32 - 1);

        delay.min(self.config.retry_config.max_delay_ms as f64) as u64
    }

    /// Reset shard iterator for a stream (useful for error recovery)
    pub async fn reset_shard_iterator(&mut self, stream_name: &str) {
        warn!("Resetting shard iterator for stream: {}", stream_name);
        self.shard_iterators.remove(stream_name);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kinesis::RetryConfig;

    #[tokio::test]
    async fn test_calculate_retry_delay() {
        let config = ConsumerConfig {
            application_name: "test".to_string(),
            consumer_group: "test".to_string(),
            shard_iterator_type: "LATEST".to_string(),
            max_records_per_batch: 100,
            polling_interval_ms: 1000,
            retry_config: RetryConfig {
                max_attempts: 3,
                base_delay_ms: 1000,
                max_delay_ms: 10000,
                backoff_multiplier: 2.0,
            },
        };

        let aws_config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
        let client = aws_sdk_kinesis::Client::new(&aws_config);
        let consumer = KinesisMessageConsumer::new(client, config).unwrap();

        assert_eq!(consumer.calculate_retry_delay(1), 1000);
        assert_eq!(consumer.calculate_retry_delay(2), 2000);
        assert_eq!(consumer.calculate_retry_delay(3), 4000);
    }
}
