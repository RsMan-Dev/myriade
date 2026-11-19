//! `Zombie.registerGoals`: the zombie's configuration is the base's (`templates::zombie_base`, with the turtle egg at 4) and its sun
//! burn (`hostile::SUN_BURN`). The villagers and the wandering trader (priority 3), the iron golem (3) and a baby turtle (5) wait for
//! their species.
use super::{templates::zombie_base, Def};
use crate::rules::RuleEntry;

pub static DEF: Def = Def {
    name: "minecraft:zombie",
    reference: Some(&zombie_base::DEF),
    rules: &[RuleEntry::new(&crate::hostile::SUN_BURN, &()), RuleEntry::new(&crate::loot::XP, &crate::loot::XpReward { min: 5, max: 5 })],
    ..Def::EMPTY
};
