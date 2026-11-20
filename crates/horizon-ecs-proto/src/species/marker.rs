//! `Marker` : passif, `tick` ne fait rien ; ses données restent dans `Raw`.
use crate::behaviours::Cap;
use crate::species::Def;

pub static DEF: Def = Def { name: "minecraft:marker", caps: Some(&[Cap::Transform, Cap::Passive]), ..Def::EMPTY };
