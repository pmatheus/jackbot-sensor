//! Terminal types and utilities

/// Terminal marker trait for ending streams or indicating termination
pub trait Terminal {
    /// Check if this represents a terminal state
    fn is_terminal(&self) -> bool;
}

/// Feed ended marker indicating a data feed has terminated
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FeedEnded;

impl Terminal for FeedEnded {
    fn is_terminal(&self) -> bool {
        true
    }
}

/// Unrecoverable error marker
#[derive(Debug)]
pub struct Unrecoverable {
    pub message: String,
    pub cause: Option<Box<dyn std::error::Error + Send + Sync>>,
}

impl Clone for Unrecoverable {
    fn clone(&self) -> Self {
        Self {
            message: self.message.clone(),
            cause: None, // We can't clone the boxed error, so we'll omit it
        }
    }
}

impl Unrecoverable {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            cause: None,
        }
    }
    
    pub fn with_cause(message: impl Into<String>, cause: Box<dyn std::error::Error + Send + Sync>) -> Self {
        Self {
            message: message.into(),
            cause: Some(cause),
        }
    }
}

impl std::fmt::Display for Unrecoverable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Unrecoverable error: {}", self.message)
    }
}

impl std::error::Error for Unrecoverable {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.cause.as_ref().map(|e| e.as_ref() as &(dyn std::error::Error + 'static))
    }
}

impl Terminal for Unrecoverable {
    fn is_terminal(&self) -> bool {
        true
    }
}