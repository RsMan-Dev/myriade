//! `minecraft:firework_rocket`: a flying rocket; its flight is `firework`'s, the species declares only that the origin saves it.
use super::Def;
use crate::behaviours::Cap;

pub static DEF: Def = Def { name: "minecraft:firework_rocket", caps: Some(&[Cap::Transform, Cap::Projectile]), ..Def::EMPTY };
