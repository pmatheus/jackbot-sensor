//! Index module for instrument management

pub mod error {
    use serde::{Deserialize, Serialize};
    use std::fmt;
    
    /// Error type for index operations
    #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
    pub struct IndexError {
        message: String,
    }
    
    impl IndexError {
        pub fn new(message: impl Into<String>) -> Self {
            Self {
                message: message.into(),
            }
        }
    }
    
    impl fmt::Display for IndexError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "Index error: {}", self.message)
        }
    }
    
    impl std::error::Error for IndexError {}
}

/// A key-value pair structure for indexed data
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keyed<K, V> {
    pub key: K,
    pub value: V,
}

impl<K, V> Keyed<K, V> {
    pub fn new(key: K, value: V) -> Self {
        Self { key, value }
    }
}

impl<K, V> Keyed<K, V> {
    pub fn as_ref(&self) -> &V {
        &self.value
    }
}

impl<K, V> crate::Keyed for Keyed<K, V> {
    type Key = K;
    
    fn key(&self) -> &Self::Key {
        &self.key
    }
}

/// Indexed collection of instruments
pub struct IndexedInstruments<T> {
    instruments: Vec<T>,
}

impl<T> IndexedInstruments<T> {
    pub fn new(instruments: Vec<T>) -> Self {
        Self { instruments }
    }
    
    pub fn instruments(&self) -> &[T] {
        &self.instruments
    }
}