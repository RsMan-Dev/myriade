//! `ZombifiedPiglin` (`mobs/undead.rs:36-90`): its own targets (no villager, golem or turtle), the melee at 2, the turtle egg at 4,
//! the stroll and the look, the nearest player WHEN it is angry (`neutral:angry`, the `neutral_anger` module that keeps the anger).
use crate::behaviours::{attack_turtle_egg, break_door, hurt_by, look_at_player, melee, nearest_target, neutral_anger, random_look, spear_use, stroll, Cond, Entry};
use crate::species::Def;

static ANGRY: Cond = Cond::Is("neutral:angry");

pub static DEF: Def = Def {
    name: "minecraft:zombified_piglin",
    caps: Some(super::FIRE_IMMUNE_CAPS),
    targets: &[
        Entry::new(&hurt_by::MODULE, 1, &hurt_by::Params { alert: true, ..hurt_by::Params::DEFAULT }),
        Entry::new(&nearest_target::MODULE, 2, &nearest_target::Params::PLAYER).when(&ANGRY),
    ],
    goals: &[
        Entry::new(&break_door::MODULE, 1, &break_door::Params::ZOMBIE),
        Entry::new(&neutral_anger::MODULE, 255, &neutral_anger::Params { boost: 0.05, ..neutral_anger::Params::DEFAULT }),
        Entry::new(&spear_use::MODULE, 1, &spear_use::Params::DEFAULT),
        Entry::new(&melee::MODULE, 2, &melee::Params::DEFAULT),
        Entry::new(&attack_turtle_egg::MODULE, 4, &attack_turtle_egg::Params::DEFAULT),
        Entry::new(&stroll::MODULE, 7, &stroll::Params::DEFAULT),
        Entry::new(&look_at_player::MODULE, 8, &look_at_player::Params { range: 8.0, ..look_at_player::Params::DEFAULT }),
        Entry::new(&random_look::MODULE, 8, &random_look::Params::DEFAULT),
    ],
    rules: &[
        // `ZombifiedPiglin extends Zombie`: it inherits `Zombie.doHurtTarget` (the jar has none of its own).
        crate::rules::RuleEntry::new(&crate::hostile::IGNITE_ON_HIT, &crate::hostile::ZOMBIE_FIRE),
        crate::rules::RuleEntry::new(&crate::loot::XP, &crate::loot::XpReward { min: 5, max: 5 }),
        crate::rules::RuleEntry::new(&crate::loot::BABY_DROPS, &crate::loot::BabyDrops::ZOMBIE),
        crate::rules::RuleEntry::new(&crate::reinforcements::RULE, &crate::reinforcements::Params::PIGLIN),
        crate::rules::RuleEntry::new(&crate::spawn_gear::RULE, &crate::spawn_gear::ZOMBIFIED_PIGLIN),
    ],
    not_in_peaceful: true,
    ..Def::EMPTY
};
