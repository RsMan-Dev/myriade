//! `minecraft:experience_orb`: its flight and ageing are `orb`'s; the species declares only that the origin saves it.
use super::Def;
use crate::behaviours::Cap;

pub static DEF: Def = Def { name: "minecraft:experience_orb", caps: Some(&[Cap::Transform]), ..Def::EMPTY };
