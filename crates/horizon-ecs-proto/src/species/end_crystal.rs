//! `minecraft:end_crystal`: no goals, no brain, no locomotion. Its box, its hits and its fire are `end_crystal`'s.
use super::Def;
use crate::behaviours::Cap;

pub static DEF: Def = Def {
    name: "minecraft:end_crystal",
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Passive]),
    ..Def::EMPTY
};
