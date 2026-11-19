//! `Donkey` (`mobs/equine.rs`): the horse's goals (same registration; its food and tempt tags are the horse's).
use super::{horse, Def};

pub static DEF: Def = Def { name: "minecraft:donkey", reference: Some(&horse::DEF), ..Def::EMPTY };
