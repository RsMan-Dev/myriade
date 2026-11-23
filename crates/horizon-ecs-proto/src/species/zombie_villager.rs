//! `ZombieVillager` (`mobs/undead.rs:36-90`): the zombie's goals and targets (it burns as the zombie does).
use super::{zombie, Change, Def};
use crate::behaviours::convert_in_water;

pub static DEF: Def = Def {
    load_attrs: super::templates::zombie_base::LOAD_ATTRS,
    name: "minecraft:zombie_villager",
    reference: Some(&zombie::DEF),
    rules: &[crate::rules::RuleEntry::new(&crate::give_item::GIVE, &crate::give_item::GiveParams::VILLAGER_CURE)],
    // `ZombieVillager.convertsInWater` is false (it has its own cure).
    changes: &[Change::Remove { module: &convert_in_water::MODULE, nth: 0 }],
    not_in_peaceful: true,
    draws: Some(&crate::behaviours::spawn_draw::VILLAGER_TYPE),
    sound_source: super::SoundCategory::Hostile,
    ..Def::EMPTY
};
