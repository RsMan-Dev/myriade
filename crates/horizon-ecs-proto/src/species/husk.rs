//! `Husk` (`mobs/undead.rs:36-90`): the zombie's configuration without the sun burn (it does not burn).
use super::{templates::zombie_base, Def};

pub static DEF: Def = Def { name: "minecraft:husk", reference: Some(&zombie_base::DEF), rules: &[crate::rules::RuleEntry::new(&crate::effects::ON_HIT, &crate::effects::HUSK)], ..Def::EMPTY };
