//! `Interaction` : boîte cliquable, passive (`Interact` porte la boîte et la réponse).
use crate::behaviours::Cap;
use crate::species::Def;

pub static DEF: Def = Def { name: "minecraft:interaction", caps: Some(&[Cap::Transform, Cap::Passive]), ..Def::EMPTY };
