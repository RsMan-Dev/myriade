//! `Witch` (`mobs/witch.rs:15-35`): the potion throw (`ranged_attack` with `Params::WITCH`: the potion chosen by the target), the stroll, looking; the
//! hurt-by and nearest-player targets. Drinking its own potions: `drink_potion`. Healing the raiders: `heal_raiders`. Left out:
//! the raider friendly fire, the stroll that avoids water, the gate of the nearest-player goal.
use super::Def;
use crate::behaviours::{float, hurt_by, look_at_player, nearest_target, random_look, ranged_attack, stroll, drink_potion, Entry, ambient_sound};

const RAIDERS: &[&str] = &["minecraft:pillager", "minecraft:vindicator", "minecraft:evoker", "minecraft:illusioner", "minecraft:ravager"];

pub static DEF: Def = Def {
    name: "minecraft:witch",
    targets: &[
        Entry::new(&hurt_by::MODULE, 1, &hurt_by::Params::DEFAULT),
        Entry::new(&nearest_target::MODULE, 2, &nearest_target::Params { types: RAIDERS, ..nearest_target::Params::HEAL_RAIDERS }),
        Entry::new(&nearest_target::MODULE, 3, &nearest_target::Params::WITCH_PLAYER),
    ],
    goals: &[
        Entry::new(&crate::behaviours::obtain_banner::MODULE, 1, &crate::behaviours::obtain_banner::Params::CANNOT_LEAD),
        Entry::new(&crate::behaviours::pathfind_to_raid::MODULE, 3, &crate::behaviours::pathfind_to_raid::Params::DEFAULT),
        Entry::new(&crate::behaviours::raid_move_through_village::MODULE, 4, &crate::behaviours::raid_move_through_village::Params::DEFAULT),
        Entry::new(&crate::behaviours::long_distance_patrol::MODULE, 4, &crate::behaviours::long_distance_patrol::Params::DEFAULT),
        Entry::new(&crate::behaviours::celebration::MODULE, 5, &()),
        Entry::new(&float::MODULE, 1, &float::Params::DEFAULT),
        Entry::new(&drink_potion::MODULE, 0, &drink_potion::Params::DEFAULT),
        Entry::new(&ranged_attack::MODULE, 2, &ranged_attack::Params::WITCH),
        Entry::new(&stroll::MODULE, 2, &stroll::Params::DEFAULT),
        Entry::new(&look_at_player::MODULE, 3, &look_at_player::Params { range: 8.0, ..look_at_player::Params::DEFAULT }),
        Entry::new(&random_look::MODULE, 3, &random_look::Params::DEFAULT),
        Entry::new(&ambient_sound::MODULE, 14, &ambient_sound::Params { sounds: &["entity.witch.ambient"], interval: Some(80), ..ambient_sound::Params::DEFAULT }),
    ],
    spawn: Some(crate::spawning::SpawnDef::MONSTER), despawn: Some(crate::despawn::DespawnDef::MOB),
    rules: &[crate::rules::RuleEntry::new(&crate::loot::XP, &crate::loot::XpReward { min: 5, max: 5 }), crate::rules::RuleEntry::new(&crate::raids::NO_RAID_WHEN_NATURAL, &())],
    not_in_peaceful: true,
    sound_source: super::SoundCategory::Hostile,
    ..Def::EMPTY
};
