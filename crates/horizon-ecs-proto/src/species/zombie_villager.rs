//! `ZombieVillager` (`mobs/undead.rs:36-90`): the zombie's goals and targets (it burns as the zombie does).
use super::{zombie, Def};

pub static DEF: Def = Def { name: "minecraft:zombie_villager", reference: Some(&zombie::DEF), ..Def::EMPTY };
