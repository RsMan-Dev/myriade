//! `Mule` (`mobs/equine.rs`): the horse's goals (same registration; its food and tempt tags are the horse's).
use super::{horse, Def};

pub static DEF: Def = Def { name: "minecraft:mule", reference: Some(&horse::DEF), rules: &[crate::rules::RuleEntry::new(&crate::make_mad::ANGRY, &crate::make_mad::Angry::MULE)], ..Def::EMPTY };
