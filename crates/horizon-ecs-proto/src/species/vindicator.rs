//! `Vindicator` (`illager.rs:70-95`): float, the melee at speed 1.0, the stroll at 0.6, looking at 3 blocks; the targets of the pillager with the villagers
//! seen. Left out: breaking and opening doors (`BreakDoor`, `RaiderOpenDoor`: the door modules), `HoldGroundAttack` (the raid socle), the avoidance of the
//! creaking, Johnny (the name that attacks everything), the raiders the hurt-by ignores.
use super::Def;
use crate::behaviours::{float, hurt_by, look_at_player, melee, nearest_target, pathfind_to_raid, stroll, Entry, break_door, hold_ground_attack, raider_open_door};

const TRADERS: &[&str] = &["minecraft:villager", "minecraft:wandering_trader"];

pub static DEF: Def = Def {
    name: "minecraft:vindicator",
    targets: &[
        Entry::new(&hurt_by::MODULE, 1, &hurt_by::Params { alert: true, ..hurt_by::Params::DEFAULT }),
        Entry::new(&nearest_target::MODULE, 2, &nearest_target::Params::PLAYER),
        Entry::new(&nearest_target::MODULE, 3, &nearest_target::Params { types: TRADERS, ..nearest_target::Params::DEFAULT }),
        Entry::new(&nearest_target::MODULE, 3, &nearest_target::Params { types: &["minecraft:iron_golem"], ..nearest_target::Params::DEFAULT }),
    ],
    goals: &[
        Entry::new(&hold_ground_attack::MODULE, 2, &hold_ground_attack::Params::DEFAULT),
        Entry::new(&break_door::RAID, 1, &break_door::Params::VINDICATOR),
        Entry::new(&raider_open_door::MODULE, 1, &()),
        Entry::new(&float::MODULE, 0, &float::Params::DEFAULT),
        Entry::new(&pathfind_to_raid::MODULE, 6, &pathfind_to_raid::Params::DEFAULT),
        Entry::new(&melee::MODULE, 5, &melee::Params::DEFAULT),
        Entry::new(&stroll::MODULE, 8, &stroll::Params { speed: 0.6, ..stroll::Params::DEFAULT }),
        Entry::new(&look_at_player::MODULE, 9, &look_at_player::Params { range: 3.0, ..look_at_player::Params::DEFAULT }),
    ],
    ..Def::EMPTY
};
