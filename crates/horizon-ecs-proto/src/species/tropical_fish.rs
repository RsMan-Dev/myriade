//! `TropicalFish`: a fish of the base, in a school of at most 8 (`maxSchoolSize`, `aquatic.rs:783`), at priority 5 (`FollowFlockLeader`).
//! Its patterns and colours are data of the species (`COMMON_TROPICAL`, `aquatic.rs`): not in this file yet.
use super::{templates::fish, Def};
use crate::behaviours::{follow_flock_leader, Entry};

pub static DEF: Def = Def {
    name: "minecraft:tropical_fish",
    reference: Some(&fish::DEF),
    goals: &[Entry::new(&follow_flock_leader::MODULE, 5, &follow_flock_leader::Params::DEFAULT)],
    ..Def::EMPTY
};
