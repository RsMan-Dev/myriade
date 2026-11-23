//! Shared breeding inheritance and parent-stream reward decisions.
use super::{MobWorld, Species};
use horizon_core::random::RandomSource;

pub fn draws_parent_variant(species: Species) -> bool {
    matches!(
        species,
        Species::Pig
            | Species::Cow
            | Species::Chicken
            | Species::Fox
            | Species::Cat
            | Species::Wolf
    )
}

pub fn orb_after_child(species: Species) -> bool {
    species == Species::Fox
}

