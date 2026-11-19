//! `MagmaCube`: the slime's bounds with a wait of four times as long, and armor by size (not ported). Same missing parts as the slime.
use super::Def;
use crate::behaviours::{cube_attack, float, nearest_target, Entry};
use crate::locomotion::{hop, Medium};

pub static DEF: Def = Def {
    name: "minecraft:magma_cube",
    targets: &[
        Entry::new(&nearest_target::MODULE, 1, &nearest_target::Params { interval: 10, ..nearest_target::Params::PLAYER }),
        Entry::new(&nearest_target::MODULE, 3, &nearest_target::Params { types: &["minecraft:iron_golem"], ..nearest_target::Params::DEFAULT }),
    ],
    goals: &[
        Entry::new(&float::MODULE, 1, &float::Params::DEFAULT),
        Entry::new(&cube_attack::MODULE, 2, &cube_attack::Params { bites_tiny: true, ..cube_attack::Params::DEFAULT }),
    ],
    rules: &[crate::rules::RuleEntry::new(&crate::size::SIZE, &crate::size::SizeParams::MAGMA_CUBE)],
    locomotion: &[(Medium::Ground, Entry::new(&hop::MODULE, 0, &hop::Params::SLIME))],
    ..Def::EMPTY
};
