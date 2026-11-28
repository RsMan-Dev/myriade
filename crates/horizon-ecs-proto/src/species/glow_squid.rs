//! `Glow squid`: swims by the impulse of its tentacle cycle (the swim mode with `squid`: no control, no path) along the vector its goals set: the
//! random movement (0) and the flee (1), and its glow (`glow_particles`). The ink and the animation are not ported.
use super::Def;
use crate::behaviours::{glow_particles, squid_goals as g, Cap, Entry};
use crate::locomotion::{swim, Medium};

pub static DEF: Def = Def {
    rules: &[crate::rules::RuleEntry::new(&crate::loot::XP, &crate::loot::XpReward { min: 1, max: 3 })],
    name: "minecraft:glow_squid",
    goals: &[Entry::new(&g::SQUID_RANDOM_MOVEMENT, 0, &g::Params::DEFAULT), Entry::new(&g::SQUID_FLEE, 1, &g::Params::DEFAULT), Entry::new(&glow_particles::MODULE, 0, &glow_particles::Params), Entry::new(&crate::behaviours::move_emission::MODULE, 0, &crate::behaviours::move_emission::Params { sounds: false, ..crate::behaviours::move_emission::Params::DEFAULT })],
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Locomotion]),
    locomotion: &[(Medium::Water, Entry::new(&swim::MODULE, 0, &swim::Params { needs_water: true, squid: true, ..swim::Params::DEFAULT }))],
    spawn: Some(crate::spawning::SpawnDef { category: crate::spawning::Category::UndergroundWaterCreature, place: crate::spawning::Place::InWater, rule: crate::spawning::Rule::GlowSquid, cluster: 4, far: false, peaceful: true, liquid: true, despawn: crate::despawn::DespawnDef::MOB }), despawn: Some(crate::despawn::DespawnDef::MOB),
    ..Def::EMPTY
};
