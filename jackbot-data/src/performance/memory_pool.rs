//! High-performance memory pools for frequent allocations in trading operations.

use parking_lot::Mutex;
use std::{
    collections::VecDeque,
    sync::Arc,
};

/// A high-performance memory pool for reusing allocations
pub struct MemoryPool<T> {
    available: Arc<Mutex<VecDeque<Box<T>>>>,
    factory: Box<dyn Fn() -> T + Send + Sync>,
    max_size: usize,
    current_size: Arc<std::sync::atomic::AtomicUsize>,
}

impl<T> MemoryPool<T>
where
    T: Default + Send + 'static,
{
    /// Create a new memory pool with default factory
    pub fn new(max_size: usize) -> Self {
        Self {
            available: Arc::new(Mutex::new(VecDeque::with_capacity(max_size / 2))),
            factory: Box::new(T::default),
            max_size,
            current_size: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        }
    }
    
    /// Create a new memory pool with custom factory function
    pub fn with_factory<F>(max_size: usize, factory: F) -> Self
    where
        F: Fn() -> T + Send + Sync + 'static,
    {
        Self {
            available: Arc::new(Mutex::new(VecDeque::with_capacity(max_size / 2))),
            factory: Box::new(factory),
            max_size,
            current_size: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        }
    }
    
    /// Acquire an object from the pool, creating new if none available
    pub fn acquire(&self) -> PooledObject<T> {
        let obj = {
            let mut available = self.available.lock();
            if let Some(obj) = available.pop_front() {
                obj
            } else {
                Box::new((self.factory)())
            }
        };
        
        PooledObject {
            obj: Some(obj),
            pool: self.available.clone(),
            max_size: self.max_size,
            current_size: self.current_size.clone(),
        }
    }
    
    /// Get current pool size
    pub fn size(&self) -> usize {
        self.available.lock().len()
    }
    
    /// Get total allocated objects
    pub fn total_allocated(&self) -> usize {
        self.current_size.load(std::sync::atomic::Ordering::Relaxed)
    }
}

/// A pooled object that automatically returns to the pool when dropped
pub struct PooledObject<T> {
    obj: Option<Box<T>>,
    pool: Arc<Mutex<VecDeque<Box<T>>>>,
    max_size: usize,
    current_size: Arc<std::sync::atomic::AtomicUsize>,
}

impl<T> PooledObject<T> {
    /// Get a reference to the underlying object
    pub fn as_ref(&self) -> &T {
        self.obj.as_ref().unwrap()
    }
    
    /// Get a mutable reference to the underlying object
    pub fn as_mut(&mut self) -> &mut T {
        self.obj.as_mut().unwrap()
    }
}

impl<T> std::ops::Deref for PooledObject<T> {
    type Target = T;
    
    fn deref(&self) -> &Self::Target {
        self.obj.as_ref().unwrap()
    }
}

impl<T> std::ops::DerefMut for PooledObject<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.obj.as_mut().unwrap()
    }
}

impl<T> Drop for PooledObject<T> {
    fn drop(&mut self) {
        if let Some(obj) = self.obj.take() {
            let mut pool = self.pool.lock();
            if pool.len() < self.max_size {
                pool.push_back(obj);
            } else {
                // Pool is full, just drop the object
                drop(obj);
            }
        }
    }
}

/// Specialized memory pool for order book level data
pub type OrderBookLevelPool = MemoryPool<Vec<(rust_decimal::Decimal, rust_decimal::Decimal)>>;

/// Specialized memory pool for WebSocket message buffers
pub type MessageBufferPool = MemoryPool<Vec<u8>>;

/// Global memory pools for common trading operations
pub struct GlobalPools {
    pub order_book_levels: OrderBookLevelPool,
    pub message_buffers: MessageBufferPool,
    pub price_vectors: MemoryPool<Vec<rust_decimal::Decimal>>,
}

impl GlobalPools {
    /// Initialize global pools with optimized sizes
    pub fn new() -> Self {
        Self {
            order_book_levels: MemoryPool::with_factory(1000, || Vec::with_capacity(200)),
            message_buffers: MemoryPool::with_factory(500, || Vec::with_capacity(8192)),
            price_vectors: MemoryPool::with_factory(200, || Vec::with_capacity(100)),
        }
    }
}

use std::sync::OnceLock;

/// Global memory pools instance
pub static GLOBAL_POOLS: OnceLock<GlobalPools> = OnceLock::new();

/// Get or initialize global pools
pub fn get_global_pools() -> &'static GlobalPools {
    GLOBAL_POOLS.get_or_init(|| GlobalPools::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_memory_pool_basic() {
        let pool: MemoryPool<Vec<i32>> = MemoryPool::new(10);
        let obj1 = pool.acquire();
        let obj2 = pool.acquire();
        
        // Should have allocated 2 objects
        assert_eq!(pool.size(), 0);
        
        drop(obj1);
        // Should have 1 object back in pool
        assert_eq!(pool.size(), 1);
        
        drop(obj2);
        // Should have 2 objects back in pool
        assert_eq!(pool.size(), 2);
    }
    
    #[test]
    fn test_memory_pool_reuse() {
        let pool = MemoryPool::with_factory(10, || vec![1, 2, 3]);
        
        {
            let mut obj = pool.acquire();
            obj.push(4);
            assert_eq!(*obj, vec![1, 2, 3, 4]);
        }
        
        // Object should be returned to pool
        assert_eq!(pool.size(), 1);
        
        {
            let obj = pool.acquire();
            // Should reuse the same object (but reset by factory)
            assert_eq!(*obj, vec![1, 2, 3, 4]); // Contains previous data
        }
    }
}