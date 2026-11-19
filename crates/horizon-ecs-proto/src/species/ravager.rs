//! `Ravager` (`mobs/ravager.rs`, `register`): float, the bite on the melee at speed 1.0, a stroll that avoids water, the glances at
//! players (6) and at mobs (8), and the targets: the hurt one (the raiders ignored), the nearest player, the adult villagers and
//! traders, the iron golems. The bite, the stun and the roar are the behaviours `strike_pause`, `shield_stun` and `roar`.
use crate::behaviours::{float, hunt_speed, hurt_by, look_at_player, melee, nearest_target, ravager_roar, shield_stun, strike_pause, stroll, Entry};
use crate::mount::{RideParams, RIDE};
use crate::rules::RuleEntry;
use crate::species::Def;

pub static DEF: Def = Def {
    name: "minecraft:ravager",
    targets: &[
        Entry::new(&hurt_by::MODULE, 2, &hurt_by::Params { alert: true, ignoring: &["minecraft:pillager", "minecraft:vindicator", "minecraft:evoker", "minecraft:illusioner", "minecraft:witch", "minecraft:ravager"], ..hurt_by::Params::DEFAULT }),
        Entry::new(&nearest_target::MODULE, 3, &nearest_target::Params::PLAYER),
        Entry::new(&nearest_target::MODULE, 4, &nearest_target::Params { players: false, types: &["minecraft:villager", "minecraft:wandering_trader"], predicate: Some("adult"), must_see: true, ..nearest_target::Params::DEFAULT }),
        Entry::new(&nearest_target::MODULE, 4, &nearest_target::Params { players: false, types: &["minecraft:iron_golem"], must_see: true, ..nearest_target::Params::DEFAULT }),
    ],
    goals: &[
        Entry::new(&float::MODULE, 0, &float::Params::DEFAULT),
        Entry::new(&hunt_speed::MODULE, 0, &hunt_speed::Params::DEFAULT),
        Entry::new(&strike_pause::MODULE, 0, &strike_pause::Params::DEFAULT),
        Entry::new(&shield_stun::MODULE, 0, &shield_stun::Params::DEFAULT),
        Entry::new(&ravager_roar::MODULE, 0, &ravager_roar::Params::DEFAULT),
        Entry::new(&melee::MODULE, 4, &melee::Params { speed: 1.0, follow_unseen: true, ..melee::Params::DEFAULT }),
        Entry::new(&stroll::MODULE, 5, &stroll::Params { speed: 0.4, ..stroll::Params::DEFAULT }),
        Entry::new(&look_at_player::MODULE, 6, &look_at_player::Params { range: 6.0, ..look_at_player::Params::DEFAULT }),
        Entry::new(&look_at_player::MODULE, 10, &look_at_player::Params { range: 8.0, mobs: true, ..look_at_player::Params::DEFAULT }),
    ],
    caps: Some(&[crate::behaviours::Cap::Transform, crate::behaviours::Cap::Solid, crate::behaviours::Cap::Body, crate::behaviours::Cap::Living, crate::behaviours::Cap::Goals, crate::behaviours::Cap::Head, crate::behaviours::Cap::Targets, crate::behaviours::Cap::Locomotion]),
    // The raider's seat (`Ravager.getPassengers`): a carrier that only takes creatures, one seat at the centre.
    rules: &[RuleEntry::new(&RIDE, &RideParams::CARRIER)],
    ..Def::EMPTY
};
