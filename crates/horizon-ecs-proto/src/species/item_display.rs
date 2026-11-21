//! `Display.ItemDisplay` : passif, sans but ni tick. Données : `display::{Transform, Shown, Stack}`.
use crate::behaviours::Cap;
use crate::species::Def;

pub static DEF: Def = Def { name: "minecraft:item_display", caps: Some(&[Cap::Transform, Cap::Passive, Cap::IgnoresTriggers]), ..Def::EMPTY };
