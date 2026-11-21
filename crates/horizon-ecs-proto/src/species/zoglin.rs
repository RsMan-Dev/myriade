//! `Zoglin` (`mobs/piglin.rs:1570-1590`, `register_zoglin`): the melee at an interval by age (41 adult, 16 baby: `brain.rs:97`,
//! `melee_attack_by_age(40, 15)` = `intervals(41, 16)`), the target of every creature but the creeper and the zoglin (`except`).
use crate::behaviours::{float, hurt_by, look_at_player, melee, nearest_target, stroll, Entry};
use crate::species::Def;

pub static DEF: Def = Def {
    name: "minecraft:zoglin",
    caps: Some(super::FIRE_IMMUNE_CAPS),
    targets: &[
        Entry::new(&hurt_by::MODULE, 1, &hurt_by::Params::DEFAULT),
        Entry::new(&nearest_target::MODULE, 2, &nearest_target::Params::PLAYER),
        Entry::new(&nearest_target::MODULE, 3, &nearest_target::Params { any: true, except: &["minecraft:creeper", "minecraft:zoglin"], ..nearest_target::Params::DEFAULT }),
    ],
    goals: &[
        Entry::new(&float::MODULE, 0, &float::Params::DEFAULT),
        Entry::new(&melee::MODULE, 2, &melee::Params { interval: 41, interval_baby: Some(16), follow_unseen: true, ..melee::Params::DEFAULT }),
        Entry::new(&stroll::MODULE, 5, &stroll::Params { speed: 0.4, ..stroll::Params::DEFAULT }),
        Entry::new(&look_at_player::MODULE, 6, &look_at_player::Params { range: 8.0, ..look_at_player::Params::DEFAULT }),
    ],
    // `Zoglin`: `xpReward` 5 (`Mob`'s reward, the equipment adds), a baby keeps its loot and its experience (`Monster`).
    rules: &[
        crate::rules::RuleEntry::new(&crate::loot::XP, &crate::loot::XpReward { min: 5, max: 5 }),
        crate::rules::RuleEntry::new(&crate::loot::BABY_DROPS, &crate::loot::BabyDrops::MONSTER),
    ],
    not_in_peaceful: true,
    ..Def::EMPTY
};
