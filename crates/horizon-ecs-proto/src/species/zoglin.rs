//! `Zoglin` (`mobs/piglin.rs:1570-1590`, `register_zoglin`). Left out: the melee, whose interval depends on the age (41 adult, 16 baby,
//! `brain.rs:97`: `melee.interval` does not exist), and the target of every creature but the creeper and the zoglin (no exclusion list).
use crate::behaviours::{float, hurt_by, look_at_player, nearest_target, stroll, Entry};
use crate::species::Def;

pub static DEF: Def = Def {
    name: "minecraft:zoglin",
    targets: &[
        Entry::new(&hurt_by::MODULE, 1, &hurt_by::Params::DEFAULT),
        Entry::new(&nearest_target::MODULE, 2, &nearest_target::Params::PLAYER),
        // attend : nearest_target toute créature sauf creeper et zoglin (must_see, interval 10) : pas de liste d'exclusion
    ],
    goals: &[
        Entry::new(&float::MODULE, 0, &float::Params::DEFAULT),
        // attend : melee à intervalle par âge (`melee.interval` : when is_adult 41 / when is_baby 16)
        Entry::new(&stroll::MODULE, 5, &stroll::Params { speed: 0.4 }),
        Entry::new(&look_at_player::MODULE, 6, &look_at_player::Params { range: 8.0, ..look_at_player::Params::DEFAULT }),
    ],
    ..Def::EMPTY
};
