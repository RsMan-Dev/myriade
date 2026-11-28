//! `SkeletonHorse` (`mobs/equine.rs:232`): no float, no mount panic, no tempt (it sinks), and no food tag (no breeding item).
use super::{Def, HorseGen};
use crate::mount::{RideParams, RIDE};
use crate::rules::RuleEntry;
use crate::behaviours::{run_around_like_crazy, Cap, breed, follow_parent, look_at_player, random_look, random_stand, stroll, Entry};

pub static DEF: Def = Def {
    name: "minecraft:skeleton_horse",
    goals: &[
        Entry::new(&run_around_like_crazy::MODULE, 1, &run_around_like_crazy::Params::DEFAULT),
        Entry::new(&breed::MODULE, 2, &breed::Params::DEFAULT),
        Entry::new(&follow_parent::MODULE, 4, &follow_parent::Params { speed: 1.0 }),
        Entry::new(&stroll::MODULE, 6, &stroll::Params { speed: 0.7, ..stroll::Params::DEFAULT }),
        Entry::new(&look_at_player::MODULE, 7, &look_at_player::Params::DEFAULT),
        Entry::new(&random_look::MODULE, 8, &random_look::Params::DEFAULT),
        Entry::new(&random_stand::MODULE, 9, &random_stand::Params::DEFAULT),
    ],
    rules: &[crate::rules::RuleEntry::new(&crate::loot::XP, &crate::loot::XpReward { min: 1, max: 3 }), crate::rules::RuleEntry::new(&crate::rules::pushable::UNPUSHABLE, &()), crate::rules::RuleEntry::new(&crate::rules::climbable::NO_CLIMB, &()), RuleEntry::new(&RIDE, &RideParams::HORSE),RuleEntry::new(&crate::spring_trap::RULE, &crate::spring_trap::Params::SKELETON_TRAP), RuleEntry::new(&crate::leash::QUAD_LEASHEE, &crate::leash::QuadParams { front: 0.04, front_back: 0.52, left_right: 0.23, up: 0.87 })],
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Head, Cap::Targets, Cap::Locomotion, Cap::Tamable, Cap::Rideable]),
    horse_gen: HorseGen::Skeleton,
    ..Def::EMPTY
};
