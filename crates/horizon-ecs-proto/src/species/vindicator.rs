//! `Vindicator` (`illager.rs:70-95`, the raider's goals of `raider.rs:140-153`): the raid walk (banner, path to the raid, village, patrol, cheer), float,
//! the avoidance of the creaking at 8 blocks (1.0, 1.2), breaking and opening doors, `HoldGroundAttack`, the melee at 1.0, the stroll at 0.6, the look
//! at players within 3 blocks (every ask) and at creatures within 8; the targets of the pillager with the villagers seen.
//! Johnny (a vindicator named Johnny, latched by `custom_name`) hunts every living creature that is not an illager.
use super::{evoker::RAIDERS, Def};
use crate::behaviours::{avoid_entity, float, hurt_by, look_at_player, melee, nearest_target, pathfind_to_raid, stroll, Entry, break_door, hold_ground_attack, raider_open_door};
use bevy_ecs::prelude::{Entity, World};

const TRADERS: &[&str] = &["minecraft:villager", "minecraft:wandering_trader"];
/// The Johnny search only: the latch of the name (`Vindicator.extra.johnny`).
fn johnny(w: &World, e: Entity) -> bool {
    w.get::<crate::custom_name::Johnny>(e).is_some()
}

pub static DEF: Def = Def {
    name: "minecraft:vindicator",
    targets: &[
        Entry::new(&hurt_by::MODULE, 1, &hurt_by::Params { alert: true, ignoring: &["minecraft:pillager", "minecraft:vindicator", "minecraft:witch", "minecraft:evoker", "minecraft:illusioner", "minecraft:vex", "minecraft:ravager"], ..hurt_by::Params::DEFAULT }),
        Entry::new(&nearest_target::MODULE, 2, &nearest_target::Params::PLAYER),
        Entry::new(&nearest_target::MODULE, 3, &nearest_target::Params { types: TRADERS, ..nearest_target::Params::DEFAULT }),
        Entry::new(&nearest_target::MODULE, 3, &nearest_target::Params { types: &["minecraft:iron_golem"], ..nearest_target::Params::DEFAULT }),
        Entry::new(&nearest_target::MODULE, 4, &nearest_target::Params { any: true, except: RAIDERS, may_search: Some(johnny), ..nearest_target::Params::DEFAULT }),
    ],
    goals: &[
        Entry::new(&crate::behaviours::obtain_banner::MODULE, 1, &crate::behaviours::obtain_banner::Params::DEFAULT),
        Entry::new(&crate::behaviours::raid_move_through_village::MODULE, 4, &crate::behaviours::raid_move_through_village::Params::DEFAULT),
        Entry::new(&crate::behaviours::long_distance_patrol::MODULE, 4, &crate::behaviours::long_distance_patrol::Params::DEFAULT),
        Entry::new(&crate::behaviours::celebration::MODULE, 5, &()),
        Entry::new(&hold_ground_attack::MODULE, 4, &hold_ground_attack::Params::VINDICATOR),
        Entry::new(&break_door::RAID, 2, &break_door::Params::VINDICATOR),
        Entry::new(&raider_open_door::MODULE, 3, &()),
        Entry::new(&float::MODULE, 0, &float::Params::DEFAULT),
        Entry::new(&avoid_entity::MODULE, 1, &avoid_entity::Params { what: avoid_entity::Avoid::Named(&["minecraft:creaking"]), max_dist: 8.0, walk: 1.0, sprint: 1.2, ..avoid_entity::Params::DEFAULT }),
        Entry::new(&pathfind_to_raid::MODULE, 3, &pathfind_to_raid::Params::DEFAULT),
        Entry::new(&melee::MODULE, 5, &melee::Params::DEFAULT),
        Entry::new(&stroll::MODULE, 8, &stroll::Params { speed: 0.6, ..stroll::Params::DEFAULT }),
        Entry::new(&look_at_player::MODULE, 9, &look_at_player::Params { range: 3.0, period: 1, ..look_at_player::Params::DEFAULT }),
        Entry::new(&look_at_player::MODULE, 10, &look_at_player::Params { range: 8.0, species: look_at_player::Params::ALL, ..look_at_player::Params::DEFAULT }),
    ],
    rules: &[crate::rules::RuleEntry::new(&crate::loot::XP, &crate::loot::XpReward { min: 5, max: 5 }), crate::rules::RuleEntry::new(&crate::spawn_gear::RULE, &crate::spawn_gear::VINDICATOR)],
    not_in_peaceful: true,
    sound_source: super::SoundCategory::Hostile,
    ..Def::EMPTY
};
