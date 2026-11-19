//! The base of the monsters that hunt the player: the hurt-by and nearest-player target goals, then stroll and look
//! (`Monster`'s goals at the priorities of the skeleton; the zombie keeps its own list for now, one more priority).
use crate::behaviours::{hurt_by, look_at_player, nearest_target, random_look, stroll, Entry};
use crate::species::Def;

pub static DEF: Def = Def {
    name: "template:monster",
    reference: None,
    targets: &[Entry::new(&hurt_by::MODULE, 1, &hurt_by::Params::DEFAULT), Entry::new(&nearest_target::MODULE, 2, &nearest_target::Params::PLAYER)],
    goals: &[
        Entry::new(&stroll::MODULE, 5, &stroll::Params::DEFAULT),
        Entry::new(&look_at_player::MODULE, 6, &look_at_player::Params { range: 8.0, ..look_at_player::Params::DEFAULT }),
        Entry::new(&random_look::MODULE, 6, &random_look::Params::DEFAULT),
    ],
    changes: &[],
    rules: &[],
    caps: None,
    brain: crate::brain::BrainDef::EMPTY,
    locomotion: &[],
    
    spawn: Some(crate::spawning::SpawnDef::MONSTER),
    despawn: Some(crate::despawn::DespawnDef::MOB),
    ..Def::EMPTY
};
