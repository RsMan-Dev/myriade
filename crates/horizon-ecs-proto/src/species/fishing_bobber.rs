//! `minecraft:fishing_bobber`: no goals, no brain, no locomotion. Its flight and its floating are `fishing_hook`'s.
use super::Def;
use crate::behaviours::Cap;

pub static DEF: Def = Def {
    name: "minecraft:fishing_bobber",
    caps: Some(&[Cap::Transform, Cap::Projectile]),
    ..Def::EMPTY
};
