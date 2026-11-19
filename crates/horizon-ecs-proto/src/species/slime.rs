//! `Slime` (size 1) as far as the frame has it: it moves by hops (`locomotion/hop.rs`, the cube's move control) and floats. Missing: the
//! attack by contact and its facing (`CubeMobAttackGoal`), the targets (the nearest player within 4 blocks of height), the size and its
//! health, speed and damage, the split into smaller ones at the death, the squish, the landing particles and the sounds.
use super::Def;
use crate::behaviours::{float, Entry};
use crate::locomotion::{hop, Medium};

pub static DEF: Def = Def {
    name: "minecraft:slime",
    reference: None,
    targets: &[],
    goals: &[Entry::new(&float::MODULE, 1, &float::Params::DEFAULT)],
    changes: &[],
    rules: &[crate::rules::RuleEntry::new(&crate::size::SIZE, &crate::size::SizeParams::SLIME)],
    caps: None,
    brain: crate::brain::BrainDef::EMPTY,
    locomotion: &[(Medium::Ground, Entry::new(&hop::MODULE, 0, &hop::Params::SLIME))],
};
