//! The base of the fish (`AbstractFish.registerGoals`): panic at 0 (1.25), the avoidance of the player at 2 (8 blocks, 1.6, 1.4) and random
//! swimming at 4. The school is added by the species that have one. A fish has no targets: no `Targets` capability. It swims in the
//! water and walks (falls, flops) out of it: both modes, one active.
use crate::behaviours::{avoid_entity, fish_swim, panic, Cap, Entry};
use crate::locomotion::{swim, walk, Medium};
use crate::species::Def;

pub static DEF: Def = Def {
    name: "template:fish",
    despawn: Some(crate::despawn::DespawnDef { removable: crate::despawn::Removable::UnlessNamed, ..crate::despawn::DespawnDef::MOB }),
    reference: None,
    targets: &[],
    goals: &[
        Entry::new(&panic::MODULE, 0, &panic::Params { speed: 1.25, ..panic::Params::DEFAULT }),
        Entry::new(&avoid_entity::MODULE, 2, &avoid_entity::Params { what: avoid_entity::Avoid::Player, max_dist: 8.0, walk: 1.6, sprint: 1.4, ..avoid_entity::Params::DEFAULT }),
        Entry::new(&fish_swim::MODULE, 4, &fish_swim::Params::DEFAULT),
    ],
    changes: &[],
    // `WaterAnimal.getBaseExperienceReward`: `1 + nextInt(3)`.
    rules: &[crate::rules::RuleEntry::new(&crate::leash::NEVER, &()), crate::rules::RuleEntry::new(&crate::loot::XP, &crate::loot::XpReward { min: 1, max: 3 })],
    brain: crate::brain::BrainDef::EMPTY,
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Locomotion]),
    locomotion: &[
        (Medium::Ground, Entry::new(&walk::MODULE, 0, &walk::Params::DEFAULT)),
        (Medium::Water, Entry::new(&swim::MODULE, 0, &swim::Params::DEFAULT)),
    ],
    
    spawn: Some(crate::spawning::SpawnDef { category: crate::spawning::Category::WaterAmbient, place: crate::spawning::Place::InWater, rule: crate::spawning::Rule::SurfaceWater, cluster: 8, far: false, peaceful: true, liquid: true, despawn: crate::despawn::DespawnDef { distance: 64.0, removable: crate::despawn::Removable::UnlessNamed, ..crate::despawn::DespawnDef::MOB } }),
    ..Def::EMPTY
};
