//! `ZombieHorse` (`mobs/equine.rs:234`): no mount panic. Its tempt has no items: it has no tag (`tempt_tag`, `food_tag`), so the
//! tempt never starts, as in the `Mob`. No food tag either (no breeding item).
use super::{Def, HorseGen};
use crate::mount::{RideParams, RIDE};
use crate::rules::RuleEntry;
use crate::behaviours::{run_around_like_crazy, Cap, breed, float, follow_parent, look_at_player, random_look, random_stand, stroll, tempt, Entry, ambient_sound};

pub static DEF: Def = Def {
    name: "minecraft:zombie_horse",
    goals: &[
        Entry::new(&run_around_like_crazy::MODULE, 1, &run_around_like_crazy::Params::DEFAULT),
        Entry::new(&float::MODULE, 0, &float::Params::DEFAULT),
        Entry::new(&breed::MODULE, 2, &breed::Params::DEFAULT),
        Entry::new(&tempt::MODULE, 3, &tempt::Params { speed: 1.25, ..tempt::Params::DEFAULT }),
        Entry::new(&follow_parent::MODULE, 4, &follow_parent::Params { speed: 1.0 }),
        Entry::new(&stroll::MODULE, 6, &stroll::Params { speed: 0.7, ..stroll::Params::DEFAULT }),
        Entry::new(&look_at_player::MODULE, 7, &look_at_player::Params::DEFAULT),
        Entry::new(&random_look::MODULE, 8, &random_look::Params::DEFAULT),
        Entry::new(&random_stand::MODULE, 9, &random_stand::Params::DEFAULT),
        Entry::new(&ambient_sound::MODULE, 14, &ambient_sound::Params { sounds: &["entity.zombie_horse.ambient"], interval: Some(400), ..ambient_sound::Params::DEFAULT }),
    ],
    rules: &[crate::rules::RuleEntry::new(&crate::loot::XP, &crate::loot::XpReward { min: 1, max: 3 }), crate::rules::RuleEntry::new(&crate::rules::pushable::UNPUSHABLE, &()), crate::rules::RuleEntry::new(&crate::rules::climbable::NO_CLIMB, &()), crate::rules::RuleEntry::new(&crate::hostile::NOT_ENEMY, &()), crate::rules::RuleEntry::new(&crate::hostile::SUN_BURN, &()), crate::rules::RuleEntry::new(&crate::hostile::SUN_BODY, &()), crate::rules::RuleEntry::new(&crate::leash::LEASH, &crate::leash::LeashParams::DEFAULT), crate::rules::RuleEntry::new(&crate::leash::QUAD_LEASHEE, &crate::leash::QuadParams { front: 0.04, front_back: 0.41, left_right: 0.18, up: 0.73 }), crate::rules::RuleEntry::new(&crate::leash::GATE, &crate::leash::GateParams(crate::leash::tamed_or_unridden)), RuleEntry::new(&RIDE, &RideParams::HORSE), RuleEntry::new(&crate::make_mad::ANGRY, &crate::make_mad::Angry::ZOMBIE_HORSE)],
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Head, Cap::Targets, Cap::Locomotion, Cap::Tamable, Cap::Rideable]),
    spawn: Some(crate::spawning::SpawnDef { cluster: 6, ..crate::spawning::SpawnDef::MONSTER }), despawn: Some(crate::despawn::DespawnDef::MOB),
    horse_gen: HorseGen::Zombie,
    ..Def::EMPTY
};
