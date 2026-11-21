//! `minecraft:area_effect_cloud`: a cloud of potion; the origin saves it only with a potion, which `effect_cloud::spawn` checks.
use super::Def;
use crate::behaviours::Cap;

pub static DEF: Def = Def { name: "minecraft:area_effect_cloud", caps: Some(&[Cap::Transform, Cap::PistonIgnored]), ..Def::EMPTY };
