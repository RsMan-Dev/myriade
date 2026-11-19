//! `Cow.registerGoals`: the base as it is.
use super::{templates::farm_animal, Def};

pub static DEF: Def = Def { name: "minecraft:cow", reference: Some(&farm_animal::DEF), ..Def::EMPTY };
