//! `Pillager` (`illager.rs:59-68`, `targets`): float, the crossbow (`ranged_crossbow`, within 8 blocks), the stroll at 0.6, looking; the targets: the
//! hurt-by with the alert, the nearest player, the villagers and wandering traders (unseen allowed), the iron golem. Left out, waiting for the raid
//! socle or for a module: `HoldGroundAttack` (priority 2: the raid), the avoidance of the creaking, `look_at_player` of the mobs (priority 10), the raiders
//! the hurt-by ignores, the crossbow in hand (supposed).
use super::Def;
use crate::behaviours::{float, hurt_by, look_at_player, nearest_target, pathfind_to_raid, ranged_crossbow, stroll, Entry, celebration, hold_ground_attack, interact_with_door, long_distance_patrol, obtain_banner, raid_move_through_village, raider_open_door};

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
        Entry::new(&raider_open_door::MODULE, 1, &()),
        Entry::new(&interact_with_door::MODULE, 2, &()),
        Entry::new(&hold_ground_attack::MODULE, 2, &hold_ground_attack::Params::DEFAULT),
        Entry::new(&obtain_banner::MODULE, 3, &obtain_banner::Params::DEFAULT),
        Entry::new(&celebration::MODULE, 4, &()),
        Entry::new(&raid_move_through_village::MODULE, 5, &raid_move_through_village::Params::DEFAULT),
        Entry::new(&long_distance_patrol::MODULE, 4, &long_distance_patrol::Params::DEFAULT),
        Entry::new(&float::MODULE, 0, &float::Params::DEFAULT),
        Entry::new(&pathfind_to_raid::MODULE, 4, &pathfind_to_raid::Params::DEFAULT),
        Entry::new(&ranged_crossbow::MODULE, 3, &ranged_crossbow::Params::PILLAGER),
        Entry::new(&stroll::MODULE, 8, &stroll::Params { speed: 0.6, ..stroll::Params::DEFAULT }),
        Entry::new(&look_at_player::MODULE, 9, &look_at_player::Params { range: 15.0, ..look_at_player::Params::DEFAULT }),
    ],
    ..Def::EMPTY
};
