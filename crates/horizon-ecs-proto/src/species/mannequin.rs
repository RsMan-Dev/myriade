//! `Mannequin` (no goals, no targets): a player-model body that does not move by itself (no `Locomotion` capability: nothing imposes the walk).
use super::Def;
use crate::behaviours::Cap;

pub static DEF: Def = Def { name: "minecraft:mannequin", caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Head]), ..Def::EMPTY };
