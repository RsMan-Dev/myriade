//! `Painting` (`painting.Painting`): a hanging entity showing a motive, sized in cells (`Size`). Its survival is `hanging.rs`.
use super::Def;
use crate::behaviours::Cap;
use bevy_ecs::prelude::*;

/// The motive's index in the motive table (the host's, by name).
#[derive(Component, Clone, Copy, Debug)]
pub struct Motive(pub u16);

pub static DEF: Def = Def {
    name: "minecraft:painting",
    caps: Some(&[Cap::Transform, Cap::Hanging]),
    ..Def::EMPTY
};
