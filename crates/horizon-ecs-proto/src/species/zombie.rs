//! `Zombie.registerGoals`: the zombie's configuration is the base's (`templates::zombie_base`, with the turtle egg at 4) and its sun
//! burn (`hostile::SUN_BURN`). The villagers and the wandering trader (priority 3), the iron golem (3) and a baby turtle (5) wait for
//! their species.
use super::{templates::zombie_base, Change, Def, Section};
use crate::behaviours::{convert_in_water, Entry};
use crate::rules::RuleEntry;

pub static DEF: Def = Def {
    name: "minecraft:zombie",
    reference: Some(&zombie_base::DEF),
    rules: &[RuleEntry::new(&crate::hostile::SUN_BURN, &()), RuleEntry::new(&crate::loot::XP, &crate::loot::XpReward { min: 5, max: 5 }), RuleEntry::new(&crate::loot::BABY_DROPS, &crate::loot::BabyDrops::ZOMBIE),RuleEntry::new(&crate::reinforcements::RULE, &crate::reinforcements::Params::ZOMBIE)],
    // `Zombie.convertsInWater`: eyes under for 600 ticks, then 300 of conversion, into a drowned.
    changes: &[Change::Add(Section::Goals, Entry::new(&convert_in_water::MODULE, 0, &convert_in_water::Params { into: "minecraft:drowned", ..convert_in_water::Params::DEFAULT }))],
    not_in_peaceful: true,
    ..Def::EMPTY
};
