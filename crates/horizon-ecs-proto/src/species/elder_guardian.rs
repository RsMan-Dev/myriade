//! `ElderGuardian` (`mobs/guardian.rs:53`): the guardian's configuration, its stroll every 400 ticks instead of 80.
use super::{guardian, Change, Def};
use crate::behaviours::stroll;

pub static DEF: Def = Def {
    name: "minecraft:elder_guardian",
    reference: Some(&guardian::DEF),
    changes: &[Change::Set { module: &stroll::MODULE, nth: 0, params: &stroll::Params { interval: Some(400), water_avoiding: 1.0, ..stroll::Params::DEFAULT } }],
    ..Def::EMPTY
};
