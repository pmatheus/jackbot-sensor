//! Safe ring buffer implementation for high-performance trading data.

use crossbeam::channel::{bounded, unbounded, Receiver, Sender, TryRecvError};
use parking_lot::RwLock;
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

/// A thread-safe ring buffer using safe Rust constructs
pub struct SafeRingBuffer<T> {
    buffer: Arc<parking_lot::Mutex<VecDeque<T>>>,
    capacity: usize,
    dropped_count: Arc<AtomicUsize>,
}

impl<T> SafeRingBuffer<T> {
    /// Create a new safe ring buffer with specified capacity
    pub fn new(capacity: usize) -> Self {
        Self {
            buffer: Arc::new(parking_lot::Mutex::new(VecDeque::with_capacity(capacity))),
            capacity,
            dropped_count: Arc::new(AtomicUsize::new(0)),
        }
    }
    
    /// Push an item into the buffer, dropping oldest if full
    pub fn push(&self, item: T) -> Result<(), T> {
        let mut buffer = self.buffer.lock();
        
        if buffer.len() >= self.capacity {
            // Drop oldest item
            buffer.pop_front();
            self.dropped_count.fetch_add(1, Ordering::Relaxed);
        }
        
        buffer.push_back(item);
        Ok(())
    }
    
    /// Try to push an item without blocking
    pub fn try_push(&self, item: T) -> Result<(), T> {
        match self.buffer.try_lock() {
            Some(mut buffer) => {
                if buffer.len() >= self.capacity {
                    buffer.pop_front();
                    self.dropped_count.fetch_add(1, Ordering::Relaxed);
                }
                buffer.push_back(item);
                Ok(())
            }
            None => Err(item), // Could not acquire lock
        }
    }
    
    /// Pop the oldest item from the buffer
    pub fn pop(&self) -> Option<T> {
        self.buffer.lock().pop_front()
    }
    
    /// Try to pop an item without blocking
    pub fn try_pop(&self) -> Option<T> {
        self.buffer.try_lock()?.pop_front()
    }
    
    /// Get current buffer size
    pub fn len(&self) -> usize {
        self.buffer.lock().len()
    }
    
    /// Check if buffer is empty
    pub fn is_empty(&self) -> bool {
        self.buffer.lock().is_empty()
    }
    
    /// Get buffer statistics
    pub fn stats(&self) -> BufferStats {
        let current_size = self.len();
        BufferStats {
            capacity: self.capacity,
            current_size,
            dropped_count: self.dropped_count.load(Ordering::Relaxed),
            utilization: (current_size as f64 / self.capacity as f64) * 100.0,
        }
    }
}

impl<T> Clone for SafeRingBuffer<T> {
    fn clone(&self) -> Self {
        Self {
            buffer: self.buffer.clone(),
            capacity: self.capacity,
            dropped_count: self.dropped_count.clone(),
        }
    }
}

/// High-performance ring buffer for market data with timestamping
#[derive(Debug)]
pub struct TimestampedRingBuffer<T> {
    entries: Arc<parking_lot::Mutex<VecDeque<TimestampedEntry<T>>>>,
    capacity: usize,
    dropped_count: Arc<AtomicUsize>,
    sequence_counter: Arc<AtomicUsize>,
}

#[derive(Debug, Clone)]
struct TimestampedEntry<T> {
    data: T,
    timestamp: Instant,
    sequence: u64,
}

impl<T> TimestampedRingBuffer<T> {
    /// Create a new timestamped ring buffer
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: Arc::new(parking_lot::Mutex::new(VecDeque::with_capacity(capacity))),
            capacity,
            dropped_count: Arc::new(AtomicUsize::new(0)),
            sequence_counter: Arc::new(AtomicUsize::new(0)),
        }
    }
    
    /// Push an item with automatic timestamping and sequencing
    pub fn push(&self, item: T) -> Result<(), T> {
        let mut entries = self.entries.lock();
        
        if entries.len() >= self.capacity {
            // Drop oldest entry
            entries.pop_front();
            self.dropped_count.fetch_add(1, Ordering::Relaxed);
        }
        
        let sequence = self.sequence_counter.fetch_add(1, Ordering::Relaxed) as u64;
        let entry = TimestampedEntry {
            data: item,
            timestamp: Instant::now(),
            sequence,
        };
        
        entries.push_back(entry);
        Ok(())
    }
    
    /// Pop the oldest item with its metadata
    pub fn pop(&self) -> Option<(T, Instant, u64)> {
        let mut entries = self.entries.lock();
        entries.pop_front().map(|entry| (entry.data, entry.timestamp, entry.sequence))
    }
    
    /// Get current buffer statistics
    pub fn stats(&self) -> BufferStats {
        let current_size = self.entries.lock().len();
        
        BufferStats {
            capacity: self.capacity,
            current_size,
            dropped_count: self.dropped_count.load(Ordering::Relaxed),
            utilization: (current_size as f64 / self.capacity as f64) * 100.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct BufferStats {
    pub capacity: usize,
    pub current_size: usize,
    pub dropped_count: usize,
    pub utilization: f64,
}

/// High-performance channel-based ring buffer for producer-consumer scenarios
pub struct ChannelRingBuffer<T> {
    sender: Sender<T>,
    receiver: Receiver<T>,
    capacity: usize,
    metrics: Arc<RwLock<ChannelMetrics>>,
}

#[derive(Debug, Default, Clone)]
pub struct ChannelMetrics {
    pub messages_sent: u64,
    pub messages_received: u64,
    pub messages_dropped: u64,
    pub last_send_time: Option<Instant>,
    pub last_receive_time: Option<Instant>,
}

impl<T> ChannelRingBuffer<T> {
    /// Create a new channel-based ring buffer
    pub fn new(capacity: usize) -> Self {
        let (sender, receiver) = bounded(capacity);
        
        Self {
            sender,
            receiver,
            capacity,
            metrics: Arc::new(RwLock::new(ChannelMetrics::default())),
        }
    }
    
    /// Create an unbounded channel buffer
    pub fn unbounded() -> Self {
        let (sender, receiver) = unbounded();
        
        Self {
            sender,
            receiver,
            capacity: usize::MAX,
            metrics: Arc::new(RwLock::new(ChannelMetrics::default())),
        }
    }
    
    /// Send a message, dropping oldest if full
    pub fn send(&self, item: T) -> Result<(), T> {
        {
            let mut metrics = self.metrics.write();
            metrics.messages_sent += 1;
            metrics.last_send_time = Some(Instant::now());
        }
        
        match self.sender.try_send(item) {
            Ok(()) => Ok(()),
            Err(crossbeam::channel::TrySendError::Full(item)) => {
                // Try to make space by consuming one item
                if let Ok(_) = self.receiver.try_recv() {
                    self.metrics.write().messages_dropped += 1;
                    // Try sending again
                    self.sender.try_send(item).map_err(|e| match e {
                        crossbeam::channel::TrySendError::Full(item) | 
                        crossbeam::channel::TrySendError::Disconnected(item) => item,
                    })
                } else {
                    Err(item)
                }
            }
            Err(crossbeam::channel::TrySendError::Disconnected(item)) => Err(item),
        }
    }
    
    /// Receive a message
    pub fn recv(&self) -> Option<T> {
        match self.receiver.try_recv() {
            Ok(item) => {
                let mut metrics = self.metrics.write();
                metrics.messages_received += 1;
                metrics.last_receive_time = Some(Instant::now());
                Some(item)
            }
            Err(TryRecvError::Empty | TryRecvError::Disconnected) => None,
        }
    }
    
    /// Receive with timeout
    pub fn recv_timeout(&self, timeout: Duration) -> Option<T> {
        match self.receiver.recv_timeout(timeout) {
            Ok(item) => {
                let mut metrics = self.metrics.write();
                metrics.messages_received += 1;
                metrics.last_receive_time = Some(Instant::now());
                Some(item)
            }
            Err(_) => None,
        }
    }
    
    /// Get buffer metrics
    pub fn metrics(&self) -> ChannelMetrics {
        self.metrics.read().clone()
    }
    
    /// Get sender clone for multi-producer scenarios
    pub fn sender(&self) -> Sender<T> {
        self.sender.clone()
    }
    
    /// Get receiver clone
    pub fn receiver(&self) -> Receiver<T> {
        self.receiver.clone()
    }
}

impl<T> Clone for ChannelRingBuffer<T> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
            receiver: self.receiver.clone(),
            capacity: self.capacity,
            metrics: self.metrics.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_safe_ring_buffer() {
        let buffer = SafeRingBuffer::new(3);
        
        assert!(buffer.is_empty());
        
        // Fill buffer
        assert!(buffer.push(1).is_ok());
        assert!(buffer.push(2).is_ok());
        assert!(buffer.push(3).is_ok());
        
        assert_eq!(buffer.len(), 3);
        
        // Exceed capacity - should drop oldest
        assert!(buffer.push(4).is_ok());
        assert_eq!(buffer.len(), 3);
        
        // Should have dropped item 1
        assert_eq!(buffer.pop(), Some(2));
        assert_eq!(buffer.pop(), Some(3));
        assert_eq!(buffer.pop(), Some(4));
        assert_eq!(buffer.pop(), None);
        
        let stats = buffer.stats();
        assert_eq!(stats.dropped_count, 1);
    }
    
    #[test]
    fn test_timestamped_ring_buffer() {
        let buffer = TimestampedRingBuffer::new(4);
        
        // Push items
        assert!(buffer.push("item1").is_ok());
        assert!(buffer.push("item2").is_ok());
        
        // Pop items
        if let Some((data, _timestamp, sequence)) = buffer.pop() {
            assert_eq!(data, "item1");
            assert_eq!(sequence, 0);
        }
        
        let stats = buffer.stats();
        assert_eq!(stats.capacity, 4);
        assert_eq!(stats.current_size, 1);
    }
    
    #[test]
    fn test_channel_ring_buffer() {
        let buffer = ChannelRingBuffer::new(2);
        
        assert!(buffer.send(1).is_ok());
        assert!(buffer.send(2).is_ok());
        
        assert_eq!(buffer.recv(), Some(1));
        assert_eq!(buffer.recv(), Some(2));
        assert_eq!(buffer.recv(), None);
        
        let metrics = buffer.metrics();
        assert_eq!(metrics.messages_sent, 2);
        assert_eq!(metrics.messages_received, 2);
    }
}