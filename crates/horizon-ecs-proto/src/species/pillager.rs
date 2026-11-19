//! `Pillager` (`illager.rs:59-68`, `targets`): float, the crossbow (`ranged_crossbow`, within 8 blocks), the stroll at 0.6, looking; the targets: the
//! hurt-by with the alert, the nearest player, the villagers and wandering traders (unseen allowed), the iron golem. Left out (`pathfind_to_raid`, priority 4: lot of the village and raid porter, pending), waiting for the raid
//! socle or for a module: `HoldGroundAttack` (priority 2: the raid), the avoidance of the creaking, `look_at_player` of the mobs (priority 10), the raiders
//! the hurt-by ignores, the crossbow in hand (supposed).
use super::Def;
use crate::behaviours::{float, hurt_by, look_at_player, nearest_target, ranged_crossbow, stroll, Entry};

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
        Entry::new(&float::MODULE, 0, &float::Params::DEFAULT),
        Entry::new(&ranged_crossbow::MODULE, 3, &ranged_crossbow::Params::PILLAGER),
        Entry::new(&stroll::MODULE, 8, &stroll::Params { speed: 0.6, ..stroll::Params::DEFAULT }),
        Entry::new(&look_at_player::MODULE, 9, &look_at_player::Params { range: 15.0, ..look_at_player::Params::DEFAULT }),
    ],
    ..Def::EMPTY
};
