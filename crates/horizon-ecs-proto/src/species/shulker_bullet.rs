//! `ShulkerBullet`: the projectile's entity type, with the capability of a projectile and nothing else (flight in `homing_projectile`, ends in `shulker_bullet.rs`).
use super::Def;
use crate::behaviours::Cap;

pub static DEF: Def = Def { name: "minecraft:shulker_bullet", caps: Some(&[Cap::Transform, Cap::Projectile]), ..Def::EMPTY };
