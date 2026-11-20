//! `Slime` (size 1) as far as the frame has it: it moves by hops (`locomotion/hop.rs`, the cube's move control) and floats. Targets:
//! the iron golem at 3 (`cube.rs:47`, `NearestTarget::species(IronGolem, true)`) and the nearest player at 1 (`cube.rs:37-44`, interval 10,
//! only within 4 blocks of height: `max_dy`). Missing: the attack by
//! contact and its facing (`CubeMobAttackGoal`), the size and its health, speed and damage, the split into smaller ones at the
//! death, the squish, the landing particles and the sounds.
use super::Def;
use crate::behaviours::{cube_attack, float, nearest_target, Entry};
use crate::locomotion::{hop, Medium};

pub static DEF: Def = Def {
    name: "minecraft:slime",
    reference: None,
    targets: &[
        Entry::new(&nearest_target::MODULE, 1, &nearest_target::Params { interval: 10, max_dy: Some(4.0), ..nearest_target::Params::PLAYER }),
        Entry::new(&nearest_target::MODULE, 3, &nearest_target::Params { types: &["minecraft:iron_golem"], ..nearest_target::Params::DEFAULT }),
    ],
    goals: &[
        Entry::new(&float::MODULE, 1, &float::Params::DEFAULT),
        Entry::new(&cube_attack::MODULE, 2, &cube_attack::Params { bites_tiny: false, ..cube_attack::Params::DEFAULT }),
    ],
    changes: &[],
    rules: &[crate::rules::RuleEntry::new(&crate::size::SIZE, &crate::size::SizeParams::SLIME)],
    caps: None,
    brain: crate::brain::BrainDef::EMPTY,
    locomotion: &[(Medium::Ground, Entry::new(&hop::MODULE, 0, &hop::Params::SLIME))],
    spawn: Some(crate::spawning::SpawnDef { rule: crate::spawning::Rule::Slime, ..crate::spawning::SpawnDef::MONSTER }), despawn: Some(crate::despawn::DespawnDef::MOB),
    ..Def::EMPTY
};
