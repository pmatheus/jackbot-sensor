//! Rate limiting implementation for API requests

use std::time::{Duration, Instant};
use std::collections::HashMap;
use tokio::sync::Mutex;

/// Priority levels for rate limiting
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Priority {
    High,
    Medium,
    Low,
}

/// Rate limiter for API requests
#[derive(Debug)]
pub struct RateLimiter {
    limits: HashMap<Priority, (u32, Duration)>, // (requests, per_duration)
    last_requests: Mutex<HashMap<Priority, Vec<Instant>>>,
}

impl RateLimiter {
    /// Create a new rate limiter with default limits
    pub fn new() -> Self {
        let mut limits = HashMap::new();
        limits.insert(Priority::High, (1200, Duration::from_secs(60))); // 1200/min
        limits.insert(Priority::Medium, (600, Duration::from_secs(60))); // 600/min  
        limits.insert(Priority::Low, (100, Duration::from_secs(60))); // 100/min
        
        Self {
            limits,
            last_requests: Mutex::new(HashMap::new()),
        }
    }
    
    /// Check if a request can be made for the given priority
    pub async fn can_make_request(&self, priority: Priority) -> bool {
        let mut requests = self.last_requests.lock().await;
        let default_limit = (100, Duration::from_secs(60));
        let (limit, duration) = self.limits.get(&priority).unwrap_or(&default_limit);
        
        let now = Instant::now();
        let cutoff = now - *duration;
        
        // Get and clean old requests
        let request_times = requests.entry(priority).or_insert_with(Vec::new);
        request_times.retain(|&time| time > cutoff);
        
        request_times.len() < *limit as usize
    }
    
    /// Record that a request was made
    pub async fn record_request(&self, priority: Priority) {
        let mut requests = self.last_requests.lock().await;
        let request_times = requests.entry(priority).or_insert_with(Vec::new);
        request_times.push(Instant::now());
    }
    
    /// Wait until a request can be made
    pub async fn wait_for_request(&self, priority: Priority) {
        while !self.can_make_request(priority).await {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
    
    /// Acquire permission to make a request
    pub async fn acquire(&self, priority: Priority) -> bool {
        if self.can_make_request(priority).await {
            self.record_request(priority).await;
            true
        } else {
            false
        }
    }
    
    /// Report a rate limit violation
    pub async fn report_violation(&self, _priority: Priority) {
        // For now, just log the violation
        // In a full implementation, this could trigger adaptive rate limiting
        tracing::warn!("Rate limit violation reported");
    }
}

impl Clone for RateLimiter {
    fn clone(&self) -> Self {
        Self {
            limits: self.limits.clone(),
            last_requests: Mutex::new(HashMap::new()),
        }
    }
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new()
    }
}