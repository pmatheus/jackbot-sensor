//! Validation utilities for WebSocket messages

use crate::error::SocketError;

/// Trait for validating WebSocket messages and data
pub trait Validator {
    type Item;
    
    /// Validate an item, returning an error if validation fails
    fn validate(&self, item: &Self::Item) -> Result<(), SocketError>;
}

/// Simple pass-through validator that accepts all items
pub struct NoOpValidator<T> {
    _marker: std::marker::PhantomData<T>,
}

impl<T> NoOpValidator<T> {
    pub fn new() -> Self {
        Self {
            _marker: std::marker::PhantomData,
        }
    }
}

impl<T> Validator for NoOpValidator<T> {
    type Item = T;
    
    fn validate(&self, _item: &Self::Item) -> Result<(), SocketError> {
        Ok(())
    }
}