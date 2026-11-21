//! `ZombieVillager` (`mobs/undead.rs:36-90`): the zombie's goals and targets (it burns as the zombie does).
use super::{zombie, Def};

pub static DEF: Def = Def {
    name: "minecraft:zombie_villager",
    reference: Some(&zombie::DEF),
    rules: &[crate::rules::RuleEntry::new(&crate::give_item::GIVE, &crate::give_item::GiveParams::VILLAGER_CURE)],
    not_in_peaceful: true,
    draws: Some(&crate::behaviours::spawn_draw::VILLAGER_TYPE),
    ..Def::EMPTY
};
