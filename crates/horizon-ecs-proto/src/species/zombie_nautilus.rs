//! `ZombieNautilus` (`mobs/nautilus.rs:55-61`, `ZombieNautilusAi`): the nautilus's swim, no panic and no love; the tempt at 0.9 (1), the
//! charge at 0.5 (2), the swimming stroll every tick (3). Its variant (temperate or warm) and its drying out are not goals.
use super::{nautilus, Def};
use crate::behaviours::{charge_attack, fish_swim, tempt, Entry};

pub static DEF: Def = Def {
    name: "minecraft:zombie_nautilus",
    variant: Some(&super::variants::ZOMBIE_NAUTILUS),
    goals: &[
        Entry::new(&tempt::MODULE, 1, &tempt::Params { speed: 0.9, items: nautilus::FOOD, stop: 3.5, ..tempt::Params::DEFAULT }),
        Entry::new(&charge_attack::MODULE, 2, &charge_attack::Params { speed: 0.5, ..charge_attack::Params::DEFAULT }),
        Entry::new(&fish_swim::MODULE, 3, &fish_swim::Params { speed: 1.0, interval: 1 }),
    ],
    caps: Some(nautilus::CAPS),
    locomotion: nautilus::LOCOMOTION,
    rules: RULES,
    ..Def::EMPTY
};
/// The nautilus's rules with its own dash sounds.
const RULES: &[crate::rules::RuleEntry] = &[
    crate::rules::RuleEntry::new(&crate::hostile::NOT_ENEMY, &()),
    crate::rules::RuleEntry::new(&crate::keep_on_use::KEEP_ON_USE, &()),
    crate::rules::RuleEntry::new(&crate::tame::TAME, &crate::tame::TameParams::NAUTILUS),
    crate::rules::RuleEntry::new(&crate::heal_by_food::HEAL_BY_FOOD, &crate::heal_by_food::HealParams::NAUTILUS),
    crate::rules::RuleEntry::new(&crate::equipment::EQUIP, &crate::equipment::EquipParams::TAME_SADDLE_BODY),
    crate::rules::RuleEntry::new(&crate::mount::RIDE, &crate::mount::RideParams::ZOMBIE_NAUTILUS),
    crate::rules::RuleEntry::new(&crate::leash::LEASH, &crate::leash::LeashParams::DEFAULT),
];
