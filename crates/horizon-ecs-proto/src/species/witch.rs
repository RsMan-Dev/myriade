//! `Witch` (`mobs/witch.rs:15-35`): the potion throw (`ranged_attack` with `Params::WITCH`: the potion chosen by the target), the stroll, looking; the
//! hurt-by and nearest-player targets. Drinking its own potions: `drink_potion`. Left out: healing the raiders,
//! the raider friendly fire, the stroll that avoids water, the gate of the nearest-player goal.
use super::Def;
use crate::behaviours::{float, hurt_by, look_at_player, nearest_target, random_look, ranged_attack, stroll, drink_potion, Entry};

pub static DEF: Def = Def {
    name: "minecraft:witch",
    targets: &[Entry::new(&hurt_by::MODULE, 1, &hurt_by::Params::DEFAULT), Entry::new(&nearest_target::MODULE, 3, &nearest_target::Params::PLAYER)],
    goals: &[
        Entry::new(&float::MODULE, 1, &float::Params::DEFAULT),
        Entry::new(&drink_potion::MODULE, 0, &drink_potion::Params::DEFAULT),
        Entry::new(&ranged_attack::MODULE, 2, &ranged_attack::Params::WITCH),
        Entry::new(&stroll::MODULE, 2, &stroll::Params::DEFAULT),
        Entry::new(&look_at_player::MODULE, 3, &look_at_player::Params { range: 8.0, ..look_at_player::Params::DEFAULT }),
        Entry::new(&random_look::MODULE, 3, &random_look::Params::DEFAULT),
    ],
    spawn: Some(crate::spawning::SpawnDef::MONSTER), despawn: Some(crate::despawn::DespawnDef::MOB),
    ..Def::EMPTY
};
