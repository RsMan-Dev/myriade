//! `AbstractHorse` (`mobs/equine.rs:203-237`): the horse's goals. Its food and tempt items are the data tags `horse_food` and
//! `horse_tempt_items` (`resources/data/minecraft/tags/item/`). Left out: `RunAroundLikeCrazy` (priority 1, a rider's goal: no
//! mounts in the framework yet). The mount panic is the plain panic while there is no rider (the rider check is not ported).
use super::Def;
use crate::behaviours::{breed, float, follow_parent, look_at_player, panic, random_look, random_stand, run_around_like_crazy, stroll, tempt, Cap, Cond, Entry};
use crate::mount::{RideParams, RIDE};
use crate::rules::RuleEntry;

static UNRIDDEN: Cond = Cond::Not(&Cond::Is("horizon:has_rider"));

pub static DEF: Def = Def {
    name: "minecraft:horse",
    goals: &[
        Entry::new(&float::MODULE, 0, &float::Params::DEFAULT),
        Entry::new(&panic::MODULE, 1, &panic::Params { speed: 1.2, ..panic::Params::DEFAULT }).when(&UNRIDDEN),
        Entry::new(&run_around_like_crazy::MODULE, 1, &run_around_like_crazy::Params::DEFAULT),
        Entry::new(&breed::MODULE, 2, &breed::Params { speed: 1.0, foods: &["minecraft:wheat", "minecraft:sugar", "minecraft:hay_block", "minecraft:apple", "minecraft:carrot", "minecraft:golden_carrot", "minecraft:golden_apple", "minecraft:enchanted_golden_apple"], ..breed::Params::DEFAULT }),
        Entry::new(&tempt::MODULE, 3, &tempt::Params { speed: 1.25, items: &["minecraft:golden_carrot", "minecraft:golden_apple", "minecraft:enchanted_golden_apple"], ..tempt::Params::DEFAULT }),
        Entry::new(&follow_parent::MODULE, 4, &follow_parent::Params { speed: 1.0 }),
        Entry::new(&stroll::MODULE, 6, &stroll::Params { speed: 0.7, ..stroll::Params::DEFAULT }),
        Entry::new(&look_at_player::MODULE, 7, &look_at_player::Params::DEFAULT),
        Entry::new(&random_look::MODULE, 8, &random_look::Params::DEFAULT),
        Entry::new(&random_stand::MODULE, 9, &random_stand::Params::DEFAULT),
    ],
    rules: &[RuleEntry::new(&RIDE, &RideParams::HORSE), RuleEntry::new(&crate::leash::LEASH, &crate::leash::LeashParams::DEFAULT)],
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Head, Cap::Targets, Cap::Locomotion, Cap::Tamable, Cap::Rideable]),
    spawn: Some(crate::spawning::SpawnDef { cluster: 6, ..crate::spawning::SpawnDef::ANIMAL }), despawn: Some(crate::despawn::DespawnDef::KEPT),
    ..Def::EMPTY
};
