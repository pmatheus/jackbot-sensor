//! Asset definitions and utilities

pub mod name {
    use serde::{Deserialize, Serialize};
    
    /// Internal representation of an asset name
    #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub struct AssetNameInternal(pub String);
    
    impl AssetNameInternal {
        pub fn new(name: impl Into<String>) -> Self {
            Self(name.into())
        }
    }
    
    impl std::fmt::Display for AssetNameInternal {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.0)
        }
    }
}