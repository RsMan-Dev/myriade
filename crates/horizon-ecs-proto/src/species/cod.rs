//! `Cod`: a fish of the base, in a school (`AbstractSchoolingFish`): it follows its leader at priority 5 (`FollowFlockLeader`).
use super::{templates::fish, Def};
use crate::behaviours::{follow_flock_leader, Entry};

pub static DEF: Def = Def {
    name: "minecraft:cod",
    reference: Some(&fish::DEF),
    goals: &[Entry::new(&follow_flock_leader::MODULE, 5, &follow_flock_leader::Params::DEFAULT)],
    ..Def::EMPTY
};
