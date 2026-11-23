//! `Blaze` (`nether.rs:25-150`): the burst of three small fireballs (`burst_shot`, priority 4), the stroll, looking. Left out: the slow fall and the rise toward a target above it
//! (`aiStep`, `customServerAiStep`), the stroll that avoids water, the alert of `HurtByTarget`.
use super::Def;
use crate::behaviours::{burst_shot, hurt_by, look_at_player, nearest_target, random_look, stroll, Entry};

pub static DEF: Def = Def {
    name: "minecraft:blaze",
    caps: Some(super::FIRE_IMMUNE_CAPS),
    targets: &[Entry::new(&hurt_by::MODULE, 1, &hurt_by::Params::DEFAULT), Entry::new(&nearest_target::MODULE, 2, &nearest_target::Params::PLAYER)],
    goals: &[
        Entry::new(&burst_shot::MODULE, 4, &burst_shot::Params::BLAZE),
        Entry::new(&stroll::MODULE, 7, &stroll::Params::DEFAULT),
        Entry::new(&look_at_player::MODULE, 8, &look_at_player::Params { range: 8.0, ..look_at_player::Params::DEFAULT }),
        Entry::new(&random_look::MODULE, 8, &random_look::Params::DEFAULT),
    ],
    not_in_peaceful: true,
    light_lit: true,
    // SpawnPlacements: ON_GROUND, Monster.checkAnyLightMonsterSpawnRules (= checkMobSpawnRules, no light test).
    spawn: Some(crate::spawning::SpawnDef { rule: crate::spawning::Rule::Ground, ..crate::spawning::SpawnDef::MONSTER }),
    sound_source: super::SoundCategory::Hostile,
    ..Def::EMPTY
};
