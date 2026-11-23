use crate::*;
use serde::{Deserialize, Serialize};

/// A registered game rule's typed value. Case order is part of the wasm link contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GameRuleValue {
    /// Boolean game rule value.
    Bool(bool),
    /// Signed integer game rule value.
    Int(i32),
}

impl std::fmt::Display for GameRuleValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bool(value) => write!(f, "{value}"),
            Self::Int(value) => write!(f, "{value}"),
        }
    }
}
