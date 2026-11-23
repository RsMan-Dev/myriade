//! `Mule` (`mobs/equine.rs`): the horse's goals (same registration; its food and tempt tags are the horse's).
use super::{horse, Def};

pub static DEF: Def = Def { name: "minecraft:mule", reference: Some(&horse::DEF), random_max_health: true, rules: &[crate::rules::RuleEntry::new(&crate::make_mad::ANGRY, &crate::make_mad::Angry::MULE), crate::rules::RuleEntry::new(&crate::leash::QUAD_LEASHEE, &crate::leash::QuadParams { front: 0.04, front_back: 0.41, left_right: 0.18, up: 0.73 })], ..Def::EMPTY };
