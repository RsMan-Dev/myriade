//! `Donkey` (`mobs/equine.rs`): the horse's goals (same registration; its food and tempt tags are the horse's).
use super::{horse, Change, Def};
use crate::behaviours::ambient_sound;

pub static DEF: Def = Def { name: "minecraft:donkey", reference: Some(&horse::DEF), random_max_health: true, body_wear: false, changes: &[Change::Set { module: &ambient_sound::MODULE, nth: 0, params: &ambient_sound::Params { sounds: &["entity.donkey.ambient"], interval: Some(400), ..ambient_sound::Params::DEFAULT } }], rules: &[crate::rules::RuleEntry::new(&crate::make_mad::ANGRY, &crate::make_mad::Angry::DONKEY), crate::rules::RuleEntry::new(&crate::leash::QUAD_LEASHEE, &crate::leash::QuadParams { front: 0.04, front_back: 0.41, left_right: 0.18, up: 0.73 })], ..Def::EMPTY };
