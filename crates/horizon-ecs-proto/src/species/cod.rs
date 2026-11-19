//! `Cod`: a fish of the base (`AbstractSchoolingFish`; the school is not ported).
use super::{templates::fish, Def};

pub static DEF: Def = Def { name: "minecraft:cod", reference: Some(&fish::DEF), ..Def::EMPTY };
