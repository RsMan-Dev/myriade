//! `GlowItemFrame`: an item frame that glows (its light is the only difference: `hanging.rs` keeps the kind).
use super::Def;
use crate::behaviours::Cap;

pub static DEF: Def = Def {
    name: "minecraft:glow_item_frame",
    caps: Some(&[Cap::Transform, Cap::Hanging]),
    ..Def::EMPTY
};
