//! The variant of a creature (`Fox.Type`: 0 red, 1 snow), chosen once at the spawn: the spawn group's value, else the biome tag
//! `spawns_snow_foxes` (`finalize.rs:2167-2179`). Kept as a component; the species' conditions read it through `is_snow`.
use super::*;

pub use crate::rules::variant::{SNOW, draw};

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Variant(pub u8);

/// The predicate of the species' conditions (registered in `species/predicates.rs` as `variant:snow`).
pub fn is_snow(w: &World, e: Entity) -> bool {
    w.get::<Variant>(e).is_some_and(|v| v.0 == SNOW)
}
