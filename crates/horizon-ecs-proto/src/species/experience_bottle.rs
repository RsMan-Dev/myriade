//! A thrown or shot projectile: its entity type, with the capability of a projectile and nothing else (flight in `projectile_more.rs`).
use super::Def;
use crate::behaviours::Cap;

pub static DEF: Def = Def { name: "minecraft:experience_bottle", caps: Some(&[Cap::Transform, Cap::Projectile]), ..Def::EMPTY };
