//! `MagmaCube`: the slime's bounds with a wait of four times as long, and armor by size (not ported). Same missing parts as the slime.
use super::Def;
use crate::behaviours::{float, Entry};
use crate::locomotion::{hop, Medium};

pub static DEF: Def = Def {
    name: "minecraft:magma_cube",
    goals: &[Entry::new(&float::MODULE, 1, &float::Params::DEFAULT)],
    rules: &[crate::rules::RuleEntry::new(&crate::size::SIZE, &crate::size::SizeParams::MAGMA_CUBE)],
    locomotion: &[(Medium::Ground, Entry::new(&hop::MODULE, 0, &hop::Params::SLIME))],
    ..Def::EMPTY
};
