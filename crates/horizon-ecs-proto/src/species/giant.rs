//! `Giant` (`Family::Inert`: no goals, no targets): a zombie-sized body that stands where it is put. Walking mode and gravity only.
use super::Def;

pub static DEF: Def = Def { sound_source: super::SoundCategory::Hostile, name: "minecraft:giant", not_in_peaceful: true, rules: &[crate::rules::RuleEntry::new(&crate::loot::XP, &crate::loot::XpReward { min: 5, max: 5 })], ..Def::EMPTY };
