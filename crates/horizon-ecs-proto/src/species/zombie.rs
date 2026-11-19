//! `Zombie.registerGoals`: melee 3, stroll 7, look 8, 8; the target selector's nearest player (priority 2).
use super::Def;
use crate::behaviours::{look_at_player, melee, random_look, stroll, target, Entry};

pub static DEF: Def = Def {
    name: "minecraft:zombie",
    reference: None,
    targets: &[Entry::new(&target::MODULE, 2, &target::Params::DEFAULT)],
    goals: &[
        Entry::new(&melee::MODULE, 3, &melee::Params::DEFAULT),
        Entry::new(&stroll::MODULE, 7, &stroll::Params::DEFAULT),
        Entry::new(&look_at_player::MODULE, 8, &look_at_player::Params { range: 8.0, ..look_at_player::Params::DEFAULT }),
        Entry::new(&random_look::MODULE, 8, &random_look::Params::DEFAULT),
    ],
    changes: &[],
};
