//! `Interaction` : boîte cliquable, passive. Données : `passive::Interact` (boîte, réponse, dernières attaque et utilisation, écrites par `interaction::{attack, click}`).
use crate::behaviours::Cap;
use crate::species::Def;

pub static DEF: Def = Def { name: "minecraft:interaction", caps: Some(&[Cap::Transform, Cap::Passive, Cap::IgnoresTriggers, Cap::PistonIgnored]), ..Def::EMPTY };
