//! A thrown or shot projectile: its entity type, with the capability of a projectile and nothing else (flight in `projectile_more.rs`).
use super::Def;
use crate::behaviours::Cap;

pub static DEF: Def = Def { name: "minecraft:dragon_fireball", caps: Some(&[Cap::Transform, Cap::Projectile]), ..Def::EMPTY };
