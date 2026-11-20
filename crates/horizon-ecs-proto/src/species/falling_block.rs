//! `FallingBlockEntity`: no goals, no brain. Its body and its landing are `falling_block.rs`; the species is only its capabilities.
use super::Def;
use crate::behaviours::Cap;

pub static DEF: Def = Def {
    name: "minecraft:falling_block",
    reference: None,
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body]),
    ..Def::EMPTY
};
