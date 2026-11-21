//! `Display.TextDisplay` : passif, sans but ni tick. Données : `display::{Transform, Shown, TextData}`.
use crate::behaviours::Cap;
use crate::species::Def;

pub static DEF: Def = Def { name: "minecraft:text_display", caps: Some(&[Cap::Transform, Cap::Passive, Cap::IgnoresTriggers]), ..Def::EMPTY };
