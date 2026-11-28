//! `Camel` (`myriade-engine/src/mobs/camel.rs:26-45`): the goals of the camel in the `Mob`'s order. The posture is a rule
//! (priority 255, no flag, never asked). The sitting camel refuses to move: `is_sitting` (the occupation the posture takes)
//! stands for `refuseToMove` (the stand-up included). Camel food is the tag `#camel_food` (the jar: `minecraft:cactus` alone).
use super::Def;
use crate::behaviours::spawn_draw::{Draw, Draws, Store, When};
use crate::rules::spawn_draw::Roll;
use crate::mount::{RideParams, RIDE};
use crate::rules::RuleEntry;
use crate::behaviours::{animal_panic, breed, follow_parent, float, look_at_player, posture, random_look, random_sitting, stroll, tempt, walk_to_look, Cap, Cond, Entry};

static NOT_SITTING: Cond = Cond::Not(&Cond::Is("horizon:is_sitting"));
const FOOD: &[&str] = &["#minecraft:camel_food"];

pub static DEF: Def = Def {
    spawn: Some(crate::spawning::SpawnDef { rule: crate::spawning::Rule::Bright { on: "minecraft:camels_spawnable_on" }, cluster: 6, ..crate::spawning::SpawnDef::ANIMAL }),
    name: "minecraft:camel",
    goals: &[
        Entry::new(&float::MODULE, 0, &float::Params::DEFAULT),
        Entry::new(&animal_panic::MODULE, 1, &animal_panic::Params { speed: 4.0, stand_up: true, ..animal_panic::Params::DEFAULT }),
        Entry::new(&breed::MODULE, 2, &breed::Params { speed: 1.0, foods: FOOD, ..breed::Params::DEFAULT }),
        Entry::new(&tempt::MODULE, 3, &tempt::Params { speed: 2.5, items: FOOD, ..tempt::Params::DEFAULT }),
        Entry::new(&follow_parent::MODULE, 4, &follow_parent::Params { speed: 2.5 }).when(&NOT_SITTING),
        Entry::new(&walk_to_look::MODULE, 4, &walk_to_look::Params { speed: 2.0, weight: 1, total: 4 }).when(&NOT_SITTING),
        Entry::new(&stroll::MODULE, 5, &stroll::Params { speed: 2.0, ..stroll::Params::DEFAULT }).when(&NOT_SITTING),
        Entry::new(&random_sitting::MODULE, 5, &random_sitting::Params { min_pose_time: 20, one_in: 1 }),
        Entry::new(&look_at_player::MODULE, 6, &look_at_player::Params { range: 6.0, ..look_at_player::Params::DEFAULT }),
        Entry::new(&random_look::MODULE, 7, &random_look::Params { interval: Some((150, 250)), ..random_look::Params::DEFAULT }),
        Entry::new(&posture::MODULE, 255, &posture::Params::DEFAULT),
    ],
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Head, Cap::Locomotion, Cap::Rideable]),
    rules: &[crate::rules::RuleEntry::new(&crate::loot::XP, &crate::loot::XpReward { min: 1, max: 3 }), crate::rules::RuleEntry::new(&crate::rules::pushable::UNPUSHABLE, &()), crate::rules::RuleEntry::new(&crate::rules::climbable::NO_CLIMB, &()), RuleEntry::new(&RIDE, &RideParams::CAMEL),RuleEntry::new(&crate::mount_jump::JUMP_SOUND, &crate::mount_jump::JumpSound { sound: "entity.camel.dash", volume: 1.0 }),RuleEntry::new(&crate::leash::LEASH, &crate::leash::LeashParams::DEFAULT), RuleEntry::new(&crate::leash::QUAD_LEASHEE, &crate::leash::QuadParams { front: 0.02, front_back: 0.48, left_right: 0.25, up: 0.82 }), RuleEntry::new(&crate::equipment::EQUIP, &crate::equipment::EquipParams::SADDLE)],
    draws: Some(&DRAWS),
    ..Def::EMPTY
};

/// `Camel.finalizeSpawn`: the camel stands at its birth (`resetLastPoseChangeTickToFullStand`), no draw.
static DRAWS: Draws = Draws {
    group: None,
    list: &[Draw { when: When::Before, kind: crate::behaviours::spawn_draw::Kind::Plain(Roll::Const(0)), store: Store::Call(crate::behaviours::posture::spawn_full_stand) }],
};
