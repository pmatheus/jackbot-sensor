//! Option instrument kind definitions

use serde::{Deserialize, Serialize};

/// Option instrument kind
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OptionKind {
    Call,
    Put,
}

impl std::fmt::Display for OptionKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OptionKind::Call => write!(f, "call"),
            OptionKind::Put => write!(f, "put"),
        }
    }
}