//! `SkeletonHorse` (`mobs/equine.rs:232`): no float, no mount panic, no tempt (it sinks), and no food tag (no breeding item).
use super::Def;
use crate::behaviours::{breed, follow_parent, look_at_player, random_look, random_stand, stroll, Entry};

pub static DEF: Def = Def {
    name: "minecraft:skeleton_horse",
    goals: &[
        Entry::new(&breed::MODULE, 2, &breed::Params::DEFAULT),
        Entry::new(&follow_parent::MODULE, 4, &follow_parent::Params { speed: 1.0 }),
        Entry::new(&stroll::MODULE, 6, &stroll::Params { speed: 0.7, ..stroll::Params::DEFAULT }),
        Entry::new(&look_at_player::MODULE, 7, &look_at_player::Params::DEFAULT),
        Entry::new(&random_look::MODULE, 8, &random_look::Params::DEFAULT),
        Entry::new(&random_stand::MODULE, 9, &random_stand::Params::DEFAULT),
    ],
    ..Def::EMPTY
};
