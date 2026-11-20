//! `ItemFrame` (`decoration.ItemFrame`): a hanging entity holding one item. Its state and its survival are `hanging.rs`.
use super::Def;
use crate::behaviours::Cap;

pub static DEF: Def = Def {
    name: "minecraft:item_frame",
    caps: Some(&[Cap::Transform, Cap::Hanging]),
    ..Def::EMPTY
};
