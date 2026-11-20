//! `Salmon`: a fish of the base, in a school of at most 5 (`maxSchoolSize`, `aquatic.rs:783`), at priority 5 (`FollowFlockLeader`).
use super::{templates::fish, Def};
use crate::behaviours::{follow_flock_leader, Entry};

pub static DEF: Def = Def {
    name: "minecraft:salmon",
    reference: Some(&fish::DEF),
    goals: &[Entry::new(&follow_flock_leader::MODULE, 5, &follow_flock_leader::Params { max_school: 5, ..follow_flock_leader::Params::DEFAULT })],
    spawn: Some(crate::spawning::SpawnDef { category: crate::spawning::Category::WaterAmbient, place: crate::spawning::Place::InWater, rule: crate::spawning::Rule::SurfaceWater, cluster: 5, far: false, peaceful: true, liquid: true, despawn: crate::despawn::DespawnDef { distance: 64.0, ..crate::despawn::DespawnDef::MOB } }),
    rules: &[crate::rules::RuleEntry::new(&crate::behaviours::held_swap::HELD_SWAP, &crate::behaviours::held_swap::SwapParams { from: "minecraft:water_bucket", to: "minecraft:salmon_bucket", adult_only: false, takes_creature: true })],
    ..Def::EMPTY
};
