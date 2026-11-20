//! `decoration.ArmorStand`: a living box that stands where it is put. No goals, no targets, no head, no locomotion; its state is `Stand`.
use super::Def;
use crate::behaviours::Cap;

pub static DEF: Def = Def {
    name: "minecraft:armor_stand",
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Stand]),
    ..Def::EMPTY
};
