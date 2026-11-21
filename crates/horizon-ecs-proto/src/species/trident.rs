//! `minecraft:trident`: a thrown trident; its flight is `trident`'s, the species declares only that the origin saves it.
use super::Def;
use crate::behaviours::Cap;

pub static DEF: Def = Def { name: "minecraft:trident", caps: Some(&[Cap::Transform, Cap::Projectile]), ..Def::EMPTY };
