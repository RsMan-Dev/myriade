//! `Pillager` (`illager.rs:59-68`, `targets`): float, the crossbow (`ranged_crossbow`, within 8 blocks), the stroll at 0.6, looking; the targets: the
//! hurt-by with the alert, the nearest player, the villagers and wandering traders (unseen allowed), the iron golem. Left out, waiting for the raid
//! socle or for a module: `HoldGroundAttack` (priority 2: the raid), the avoidance of the creaking, `look_at_player` of the mobs (priority 10), the raiders
//! the hurt-by ignores, the crossbow in hand (supposed).
use super::Def;
use crate::behaviours::{avoid_entity, float, hurt_by, look_at_player, nearest_target, ranged_crossbow, stroll, Entry, hold_ground_attack, pathfind_to_raid};

const TRADERS: &[&str] = &["minecraft:villager", "minecraft:wandering_trader"];

pub static DEF: Def = Def {
    name: "minecraft:pillager",
    targets: &[
        Entry::new(&hurt_by::MODULE, 1, &hurt_by::Params { alert: true, ..hurt_by::Params::DEFAULT }),
        Entry::new(&nearest_target::MODULE, 2, &nearest_target::Params::PLAYER),
        Entry::new(&nearest_target::MODULE, 3, &nearest_target::Params { types: TRADERS, must_see: false, ..nearest_target::Params::DEFAULT }),
        Entry::new(&nearest_target::MODULE, 3, &nearest_target::Params { types: &["minecraft:iron_golem"], ..nearest_target::Params::DEFAULT }),
    ],
    goals: &[
        Entry::new(&crate::behaviours::obtain_banner::MODULE, 1, &crate::behaviours::obtain_banner::Params::DEFAULT),
        Entry::new(&crate::behaviours::raid_move_through_village::MODULE, 4, &crate::behaviours::raid_move_through_village::Params::DEFAULT),
        Entry::new(&crate::behaviours::long_distance_patrol::MODULE, 4, &crate::behaviours::long_distance_patrol::Params::DEFAULT),
        Entry::new(&crate::behaviours::celebration::MODULE, 5, &()),
        Entry::new(&avoid_entity::MODULE, 1, &avoid_entity::Params { what: avoid_entity::Avoid::Named(&["minecraft:creaking"]), max_dist: 8.0, walk: 1.0, sprint: 1.2, ..avoid_entity::Params::DEFAULT }),
        Entry::new(&hold_ground_attack::MODULE, 2, &hold_ground_attack::Params::DEFAULT),
        Entry::new(&float::MODULE, 0, &float::Params::DEFAULT),
        Entry::new(&pathfind_to_raid::MODULE, 3, &pathfind_to_raid::Params::DEFAULT),
        Entry::new(&ranged_crossbow::MODULE, 3, &ranged_crossbow::Params::PILLAGER),
        Entry::new(&stroll::MODULE, 8, &stroll::Params { speed: 0.6, ..stroll::Params::DEFAULT }),
        Entry::new(&look_at_player::MODULE, 9, &look_at_player::Params { range: 15.0, ..look_at_player::Params::DEFAULT }),
        Entry::new(&look_at_player::MODULE, 10, &look_at_player::Params { range: 15.0, species: &["*"], ..look_at_player::Params::DEFAULT }),
    ],
    ..Def::EMPTY
};
