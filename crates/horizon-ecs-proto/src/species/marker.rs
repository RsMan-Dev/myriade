//! `Marker` : passif, `tick` ne fait rien ; ses clés libres restent dans `nbt::NbtData`.
use crate::behaviours::Cap;
use crate::species::Def;

pub static DEF: Def = Def { name: "minecraft:marker", caps: Some(&[Cap::Transform, Cap::Passive]), ..Def::EMPTY };
