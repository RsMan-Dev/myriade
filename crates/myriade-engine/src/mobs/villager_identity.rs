//! Borrowed profession names and command-name construction.
use crate::mobs::{Species, villager};
use std::sync::Arc;

/// The vanilla render name used by the existing numeric profession adapter.
pub fn profession_id(index: u8) -> &'static str {
    villager::profession(index).id.as_str()
}



