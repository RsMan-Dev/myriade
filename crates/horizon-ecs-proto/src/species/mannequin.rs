//! `Mannequin` (no goals, no targets): a player-model body that does not move by itself (no `Locomotion` capability: nothing imposes the walk).
//! jar : aucun but, voir ecarts (Avatar, LivingEntity, pas de brain ni de goals ; le Mob lui en donne cinq).
//! Le test strict est ignoré dans tests/it/divers_goals_vs_mob.rs.
use super::Def;
use crate::behaviours::Cap;

pub static DEF: Def = Def { name: "minecraft:mannequin", caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Head]), ..Def::EMPTY };
