//! `Salmon`: a fish of the base, in a school of at most 5 (`maxSchoolSize`, `aquatic.rs:783`), at priority 5 (`FollowFlockLeader`).
use super::{templates::fish, Def};
use crate::locomotion::{swim, walk, Medium};
use crate::behaviours::{follow_flock_leader, Entry, ambient_sound};

pub static DEF: Def = Def {
    name: "minecraft:salmon",
    variant: Some(&super::variants::SALMON),
    reference: Some(&fish::DEF),
    locomotion: &[
        (Medium::Ground, Entry::new(&walk::MODULE, 0, &walk::Params::DEFAULT)),
        (Medium::Water, Entry::new(&swim::MODULE, 0, &swim::Params { flop: Some("entity.salmon.flop"), ..swim::Params::DEFAULT })),
    ],
    goals: &[Entry::new(&follow_flock_leader::MODULE, 5, &follow_flock_leader::Params { max_school: 5, ..follow_flock_leader::Params::DEFAULT }), Entry::new(&ambient_sound::MODULE, 14, &ambient_sound::Params { sounds: &["entity.salmon.ambient"], interval: Some(120), ..ambient_sound::Params::DEFAULT })],
    spawn: Some(crate::spawning::SpawnDef { category: crate::spawning::Category::WaterAmbient, place: crate::spawning::Place::InWater, rule: crate::spawning::Rule::SurfaceWater, cluster: 5, far: false, peaceful: true, liquid: true, despawn: crate::despawn::DespawnDef { distance: 64.0, removable: crate::despawn::Removable::UnlessNamed, ..crate::despawn::DespawnDef::MOB } }),
    rules: &[crate::rules::RuleEntry::new(&crate::behaviours::held_swap::HELD_SWAP, &crate::behaviours::held_swap::SwapParams { from: "minecraft:water_bucket", to: "minecraft:salmon_bucket", adult_only: false, takes_creature: true, sound: "minecraft:item.bucket.fill_fish", alt: None, extra: crate::behaviours::held_swap::salmon_data })],
    ..Def::EMPTY
};
