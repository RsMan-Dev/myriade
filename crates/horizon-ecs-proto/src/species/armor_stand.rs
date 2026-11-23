//! `decoration.ArmorStand`: a living box that stands where it is put. No goals, no targets, no head, no locomotion; its state is `Stand`.
use super::Def;
use crate::behaviours::Cap;

pub static DEF: Def = Def {
    name: "minecraft:armor_stand",
    rules: &[crate::rules::RuleEntry::new(&crate::behaviours::vibration_listen::UNNOTICED, &())],
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Stand]),
    ..Def::EMPTY
};

/// The item that places this entity, and the one it drops when a player breaks it.
pub const ITEM: &str = "minecraft:armor_stand";
