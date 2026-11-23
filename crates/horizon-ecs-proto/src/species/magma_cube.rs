//! `MagmaCube`: the slime's bounds with a wait of four times as long, and armor by size (not ported). Targets: the iron golem at 3
//! (`cube.rs:47`, the same register as the slime). the nearest player at 1 within 4 blocks of height (`cube.rs:37-44`, `max_dy`).
use super::Def;
use crate::behaviours::{cube_attack, float, nearest_target, Entry};
use crate::locomotion::{hop, Medium};

pub static DEF: Def = Def {
    spawn: Some(crate::spawning::SpawnDef { rule: crate::spawning::Rule::Any, ..crate::spawning::SpawnDef::MONSTER }),
    name: "minecraft:magma_cube",
    caps: Some(super::FIRE_IMMUNE_CAPS),
    targets: &[
        Entry::new(&nearest_target::MODULE, 1, &nearest_target::Params { interval: 10, max_dy: Some(4.0), ..nearest_target::Params::PLAYER }),
        Entry::new(&nearest_target::MODULE, 3, &nearest_target::Params { types: &["minecraft:iron_golem"], ..nearest_target::Params::DEFAULT }),
    ],
    goals: &[
        Entry::new(&float::MODULE, 1, &float::Params::DEFAULT),
        Entry::new(&cube_attack::MODULE, 2, &cube_attack::Params { bites_tiny: true, ..cube_attack::Params::DEFAULT }),
        Entry::new(&crate::behaviours::contact_damage::MODULE, 2, &crate::behaviours::contact_damage::Params { min_size: 1, bonus: 2.0 }),
    ],
    rules: &[crate::rules::RuleEntry::new(&crate::size::SIZE, &crate::size::SizeParams::MAGMA_CUBE)],
    locomotion: &[(Medium::Ground, Entry::new(&hop::MODULE, 0, &hop::Params::SLIME))],
    not_in_peaceful: true,
    draws: Some(&crate::behaviours::spawn_draw::SPAWN_SIZE),
    sound_source: super::SoundCategory::Hostile,
    ..Def::EMPTY
};
