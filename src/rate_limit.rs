use anyhow::Result;
use governor::{Quota, RateLimiter, clock::DefaultClock, state::InMemoryState, middleware::NoOpMiddleware};
use std::collections::HashMap;
use std::sync::Arc;
// use std::time::Duration;
use std::net::IpAddr;
use tokio::sync::RwLock;
use tracing::{warn, debug, info};

// use crate::api::ErrorCode;

/// Rate limiting configuration as per API contract
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    // Public endpoints
    pub market_data_per_minute: u32,        // 100 requests/minute
    pub historical_data_per_minute: u32,    // 20 requests/minute
    
    // Authenticated endpoints
    pub orders_per_minute: u32,             // 60 requests/minute
    pub positions_per_minute: u32,          // 100 requests/minute
    pub graphql_queries_per_minute: u32,    // 1000 points/minute
    pub graphql_mutations_per_minute: u32,  // 100 points/minute
    
    // WebSocket limits
    pub ws_connections_per_user: u32,       // 5 connections
    pub ws_subscriptions_per_connection: u32, // 100 subscriptions
    pub ws_messages_per_second: u32,        // 10 messages/second
    
    // Burst allowances
    pub burst_multiplier: u32,              // Allow short bursts
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            market_data_per_minute: 100,
            historical_data_per_minute: 20,
            orders_per_minute: 60,
            positions_per_minute: 100,
            graphql_queries_per_minute: 1000,
            graphql_mutations_per_minute: 100,
            ws_connections_per_user: 5,
            ws_subscriptions_per_connection: 100,
            ws_messages_per_second: 10,
            burst_multiplier: 3,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RateLimitBucket {
    // IP-based buckets for public endpoints
    MarketData(IpAddr),
    HistoricalData(IpAddr),
    
    // User-based buckets for authenticated endpoints
    Orders(String),      // user_id
    Positions(String),   // user_id
    GraphQLQueries(String), // user_id
    GraphQLMutations(String), // user_id
    
    // WebSocket buckets
    WebSocketConnections(String), // user_id
    WebSocketMessages(String),    // connection_id
}

#[derive(Debug)]
pub struct RateLimitInfo {
    pub limit: u32,
    pub remaining: u32,
    pub reset_time: u64, // Unix timestamp
    pub bucket: String,
}

type RateLimiterType = RateLimiter<
    governor::state::NotKeyed,
    InMemoryState,
    DefaultClock,
    NoOpMiddleware<governor::clock::QuantaInstant>
>;

pub struct RateLimitManager {
    config: RateLimitConfig,
    limiters: Arc<RwLock<HashMap<RateLimitBucket, Arc<RateLimiterType>>>>,
    ws_connections: Arc<RwLock<HashMap<String, Vec<String>>>>, // user_id -> connection_ids
    ws_subscriptions: Arc<RwLock<HashMap<String, u32>>>,       // connection_id -> subscription_count
}

impl RateLimitManager {
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            config,
            limiters: Arc::new(RwLock::new(HashMap::new())),
            ws_connections: Arc::new(RwLock::new(HashMap::new())),
            ws_subscriptions: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// Check if a request is allowed for the given bucket
    pub async fn check_rate_limit(&self, bucket: RateLimitBucket) -> Result<RateLimitInfo> {
        let limiter = self.get_or_create_limiter(&bucket).await;
        
        match limiter.check() {
            Ok(_) => {
                let (limit, remaining, reset_time) = self.get_limit_info(&bucket, &limiter).await;
                
                debug!("Rate limit check passed for bucket: {:?}", bucket);
                
                Ok(RateLimitInfo {
                    limit,
                    remaining,
                    reset_time,
                    bucket: self.bucket_to_string(&bucket),
                })
            },
            Err(_) => {
                let (limit, remaining, reset_time) = self.get_limit_info(&bucket, &limiter).await;
                
                warn!("Rate limit exceeded for bucket: {:?}", bucket);
                
                Err(anyhow::anyhow!("Rate limit exceeded for {}", self.bucket_to_string(&bucket)))
            }
        }
    }
    
    /// Get or create a rate limiter for the given bucket
    async fn get_or_create_limiter(&self, bucket: &RateLimitBucket) -> Arc<RateLimiterType> {
        let limiters = self.limiters.read().await;
        
        if let Some(limiter) = limiters.get(bucket) {
            return limiter.clone();
        }
        
        drop(limiters);
        
        // Create new limiter
        let quota = self.get_quota_for_bucket(bucket);
        let limiter = Arc::new(RateLimiter::direct(quota));
        
        let mut limiters = self.limiters.write().await;
        limiters.insert(bucket.clone(), limiter.clone());
        
        info!("Created new rate limiter for bucket: {:?}", bucket);
        
        limiter
    }
    
    /// Get the appropriate quota for a rate limit bucket
    fn get_quota_for_bucket(&self, bucket: &RateLimitBucket) -> Quota {
        match bucket {
            RateLimitBucket::MarketData(_) => {
                Quota::per_minute(
                    std::num::NonZeroU32::new(self.config.market_data_per_minute * self.config.burst_multiplier).unwrap()
                )
            },
            RateLimitBucket::HistoricalData(_) => {
                Quota::per_minute(
                    std::num::NonZeroU32::new(self.config.historical_data_per_minute * self.config.burst_multiplier).unwrap()
                )
            },
            RateLimitBucket::Orders(_) => {
                Quota::per_minute(
                    std::num::NonZeroU32::new(self.config.orders_per_minute * self.config.burst_multiplier).unwrap()
                )
            },
            RateLimitBucket::Positions(_) => {
                Quota::per_minute(
                    std::num::NonZeroU32::new(self.config.positions_per_minute * self.config.burst_multiplier).unwrap()
                )
            },
            RateLimitBucket::GraphQLQueries(_) => {
                Quota::per_minute(
                    std::num::NonZeroU32::new(self.config.graphql_queries_per_minute * self.config.burst_multiplier).unwrap()
                )
            },
            RateLimitBucket::GraphQLMutations(_) => {
                Quota::per_minute(
                    std::num::NonZeroU32::new(self.config.graphql_mutations_per_minute * self.config.burst_multiplier).unwrap()
                )
            },
            RateLimitBucket::WebSocketConnections(_) => {
                // WebSocket connections are checked differently
                Quota::per_hour(
                    std::num::NonZeroU32::new(self.config.ws_connections_per_user).unwrap()
                )
            },
            RateLimitBucket::WebSocketMessages(_) => {
                Quota::per_second(
                    std::num::NonZeroU32::new(self.config.ws_messages_per_second * self.config.burst_multiplier).unwrap()
                )
            },
        }
    }
    
    /// Get current limit information
    async fn get_limit_info(
        &self,
        bucket: &RateLimitBucket,
        limiter: &RateLimiterType,
    ) -> (u32, u32, u64) {
        let quota = self.get_quota_for_bucket(bucket);
        // TODO: Fix governor quota API - max_burst() doesn't exist
        let base_limit = 100; // Temporary placeholder
        
        // This is a simplified version - in reality, we'd need to track exact remaining count
        let remaining = (base_limit as u32).saturating_sub(1);
        
        // Calculate reset time based on the quota's period
        let reset_time = match bucket {
            RateLimitBucket::WebSocketMessages(_) => {
                chrono::Utc::now().timestamp() as u64 + 1 // Reset in 1 second
            },
            RateLimitBucket::WebSocketConnections(_) => {
                chrono::Utc::now().timestamp() as u64 + 3600 // Reset in 1 hour
            },
            _ => {
                chrono::Utc::now().timestamp() as u64 + 60 // Reset in 1 minute
            }
        };
        
        (base_limit, remaining, reset_time)
    }
    
    /// Convert bucket to string for headers
    fn bucket_to_string(&self, bucket: &RateLimitBucket) -> String {
        match bucket {
            RateLimitBucket::MarketData(_) => "market-data".to_string(),
            RateLimitBucket::HistoricalData(_) => "historical-data".to_string(),
            RateLimitBucket::Orders(_) => "orders".to_string(),
            RateLimitBucket::Positions(_) => "positions".to_string(),
            RateLimitBucket::GraphQLQueries(_) => "graphql-queries".to_string(),
            RateLimitBucket::GraphQLMutations(_) => "graphql-mutations".to_string(),
            RateLimitBucket::WebSocketConnections(_) => "websocket-connections".to_string(),
            RateLimitBucket::WebSocketMessages(_) => "websocket-messages".to_string(),
        }
    }
    
    /// Check WebSocket connection limit for a user
    pub async fn check_ws_connection_limit(&self, user_id: &str) -> Result<bool> {
        let connections = self.ws_connections.read().await;
        
        if let Some(user_connections) = connections.get(user_id) {
            if user_connections.len() >= self.config.ws_connections_per_user as usize {
                warn!("WebSocket connection limit exceeded for user: {}", user_id);
                return Ok(false);
            }
        }
        
        Ok(true)
    }
    
    /// Add a WebSocket connection for a user
    pub async fn add_ws_connection(&self, user_id: &str, connection_id: &str) {
        let mut connections = self.ws_connections.write().await;
        
        connections
            .entry(user_id.to_string())
            .or_insert_with(Vec::new)
            .push(connection_id.to_string());
        
        let mut subscriptions = self.ws_subscriptions.write().await;
        subscriptions.insert(connection_id.to_string(), 0);
        
        info!("Added WebSocket connection {} for user {}", connection_id, user_id);
    }
    
    /// Remove a WebSocket connection
    pub async fn remove_ws_connection(&self, user_id: &str, connection_id: &str) {
        let mut connections = self.ws_connections.write().await;
        
        if let Some(user_connections) = connections.get_mut(user_id) {
            user_connections.retain(|id| id != connection_id);
            if user_connections.is_empty() {
                connections.remove(user_id);
            }
        }
        
        let mut subscriptions = self.ws_subscriptions.write().await;
        subscriptions.remove(connection_id);
        
        info!("Removed WebSocket connection {} for user {}", connection_id, user_id);
    }
    
    /// Check subscription limit for a WebSocket connection
    pub async fn check_ws_subscription_limit(&self, connection_id: &str, additional: u32) -> Result<bool> {
        let subscriptions = self.ws_subscriptions.read().await;
        
        if let Some(current_count) = subscriptions.get(connection_id) {
            if current_count + additional > self.config.ws_subscriptions_per_connection {
                warn!("WebSocket subscription limit exceeded for connection: {}", connection_id);
                return Ok(false);
            }
        }
        
        Ok(true)
    }
    
    /// Add subscriptions to a WebSocket connection
    pub async fn add_ws_subscriptions(&self, connection_id: &str, count: u32) {
        let mut subscriptions = self.ws_subscriptions.write().await;
        
        if let Some(current_count) = subscriptions.get_mut(connection_id) {
            *current_count += count;
        }
        
        debug!("Added {} subscriptions to connection {}", count, connection_id);
    }
    
    /// Remove subscriptions from a WebSocket connection
    pub async fn remove_ws_subscriptions(&self, connection_id: &str, count: u32) {
        let mut subscriptions = self.ws_subscriptions.write().await;
        
        if let Some(current_count) = subscriptions.get_mut(connection_id) {
            *current_count = current_count.saturating_sub(count);
        }
        
        debug!("Removed {} subscriptions from connection {}", count, connection_id);
    }
    
    /// Get rate limit statistics
    pub async fn get_stats(&self) -> serde_json::Value {
        let limiters = self.limiters.read().await;
        let connections = self.ws_connections.read().await;
        let subscriptions = self.ws_subscriptions.read().await;
        
        let total_connections: usize = connections.values().map(|v| v.len()).sum();
        let total_subscriptions: u32 = subscriptions.values().sum();
        
        serde_json::json!({
            "activeLimiters": limiters.len(),
            "totalWebSocketConnections": total_connections,
            "totalWebSocketSubscriptions": total_subscriptions,
            "uniqueUsers": connections.len(),
            "config": {
                "marketDataPerMinute": self.config.market_data_per_minute,
                "ordersPerMinute": self.config.orders_per_minute,
                "wsConnectionsPerUser": self.config.ws_connections_per_user,
                "wsSubscriptionsPerConnection": self.config.ws_subscriptions_per_connection
            }
        })
    }
    
    /// Clean up old rate limiters (call periodically)
    pub async fn cleanup_old_limiters(&self) {
        let mut limiters = self.limiters.write().await;
        
        // Remove limiters that haven't been used recently
        // This is a simplified version - in reality, you'd track last usage
        if limiters.len() > 10000 {
            limiters.clear();
            info!("Cleaned up old rate limiters");
        }
    }
}

/// Utility function to determine rate limit bucket from request path and user
pub fn get_rate_limit_bucket_from_path(
    path: &str,
    user_id: Option<&str>,
    ip: Option<IpAddr>,
) -> Option<RateLimitBucket> {
    match path {
        // Market data endpoints (IP-based)
        path if path.starts_with("/api/v1/market/") => {
            ip.map(RateLimitBucket::MarketData)
        },
        
        // Historical data endpoints (IP-based)
        path if path.starts_with("/api/v1/historical/") => {
            ip.map(RateLimitBucket::HistoricalData)
        },
        
        // Order endpoints (user-based)
        path if path.starts_with("/api/v1/orders") => {
            user_id.map(|uid| RateLimitBucket::Orders(uid.to_string()))
        },
        
        // Position endpoints (user-based)
        path if path.starts_with("/api/v1/account/positions") ||
                 path.starts_with("/api/v1/account/balances") => {
            user_id.map(|uid| RateLimitBucket::Positions(uid.to_string()))
        },
        
        // GraphQL endpoints would be handled separately
        path if path.starts_with("/graphql") => {
            // Would need to parse the query to determine if it's a query or mutation
            user_id.map(|uid| RateLimitBucket::GraphQLQueries(uid.to_string()))
        },
        
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;
    use tokio::time::{sleep, Duration};
    
    #[tokio::test]
    async fn test_rate_limiting() {
        let config = RateLimitConfig {
            market_data_per_minute: 2, // Very low for testing
            ..Default::default()
        };
        
        let manager = RateLimitManager::new(config);
        let bucket = RateLimitBucket::MarketData(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)));
        
        // First request should pass
        assert!(manager.check_rate_limit(bucket.clone()).await.is_ok());
        
        // Second request should pass
        assert!(manager.check_rate_limit(bucket.clone()).await.is_ok());
        
        // Third request should be rate limited
        assert!(manager.check_rate_limit(bucket.clone()).await.is_err());
    }
    
    #[tokio::test]
    async fn test_websocket_limits() {
        let config = RateLimitConfig {
            ws_connections_per_user: 2,
            ..Default::default()
        };
        
        let manager = RateLimitManager::new(config);
        let user_id = "test_user";
        
        // Should allow first connection
        assert!(manager.check_ws_connection_limit(user_id).await.unwrap());
        manager.add_ws_connection(user_id, "conn1").await;
        
        // Should allow second connection
        assert!(manager.check_ws_connection_limit(user_id).await.unwrap());
        manager.add_ws_connection(user_id, "conn2").await;
        
        // Should deny third connection
        assert!(!manager.check_ws_connection_limit(user_id).await.unwrap());
    }
}