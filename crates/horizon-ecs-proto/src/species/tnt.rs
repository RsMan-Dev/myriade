//! `PrimedTnt`: no goals, no brain, no walk. Its body and its fuse are `tnt.rs`; the species is only its capabilities.
use super::Def;
use crate::behaviours::Cap;

pub static DEF: Def = Def {
    name: "minecraft:tnt",
    reference: None,
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body]),
    ..Def::EMPTY
};
