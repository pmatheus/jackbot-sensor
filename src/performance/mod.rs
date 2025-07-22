//! High-performance modules for ultra-low latency trading
//!
//! This module provides optimized data structures and algorithms
//! for achieving <10ms latency in production environments.

pub mod orderbook_ultra;
pub mod cpu_affinity;

pub use orderbook_ultra::*;
pub use cpu_affinity::*;