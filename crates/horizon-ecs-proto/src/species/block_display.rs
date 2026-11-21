//! `Display.BlockDisplay` : passif, sans but ni tick. Données : `display::{Transform, Shown, BlockState}`.
use crate::behaviours::Cap;
use crate::species::Def;

pub static DEF: Def = Def { name: "minecraft:block_display", caps: Some(&[Cap::Transform, Cap::Passive, Cap::IgnoresTriggers]), ..Def::EMPTY };
