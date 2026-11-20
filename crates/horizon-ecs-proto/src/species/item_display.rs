//! `Display.ItemDisplay` : passif, sans but ni tick (le serveur ne fait rien à chaque tick).
use crate::behaviours::Cap;
use crate::species::Def;

pub static DEF: Def = Def { name: "minecraft:item_display", caps: Some(&[Cap::Transform, Cap::Passive]), ..Def::EMPTY };
