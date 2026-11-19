//! `spider` (`mobs/hostile.rs:42-60`): the walk of a WALL CLIMBER (`walk::Params.climber`: the wall is climbed, the climb goes on straight at the target
//! when the path is over). Goals: float (1), the armadillo avoidance (2, 6 / 1,0 / 1,2), leap at the target (3), melee that follows unseen
//! (4: `SpiderAttackGoal`, the melee module that gives up in light, one chance in 100 per tick, and does not start with a rider), stroll (5),
//! look (6); targets: hurt by (1), the nearest player (2) and the iron golem (3), the last two only in the dark (`horizon:dark_enough`).
//! The armadillo avoids by name, whatever its state (as the `Mob`).
use super::Def;
use crate::behaviours::{avoid_entity, Cap, Cond, float, hurt_by, leap_at_target, look_at_player, melee, nearest_target, random_look, stroll, Entry};
use crate::locomotion::{walk, Medium};
use crate::mount::{RideParams, RIDE};
use crate::rules::RuleEntry;

pub static DARK: Cond = Cond::Is("horizon:dark_enough");
pub static NO_RIDER: Cond = Cond::Not(&Cond::Is("horizon:has_rider"));
/// The nearest-player and iron-golem targets and the attack of the spiders (`cave_spider` has the same).
pub const TARGETS: &[Entry] = &[
    Entry::new(&hurt_by::MODULE, 1, &hurt_by::Params::DEFAULT),
    Entry::new(&nearest_target::MODULE, 2, &nearest_target::Params::PLAYER).when(&DARK),
    Entry::new(&nearest_target::MODULE, 3, &nearest_target::Params { types: &["minecraft:iron_golem"], ..nearest_target::Params::DEFAULT }).when(&DARK),
];
pub const ARMADILLO: Entry = Entry::new(&avoid_entity::MODULE, 2, &avoid_entity::Params { what: avoid_entity::Avoid::Named(&["minecraft:armadillo"]), max_dist: 6.0, walk: 1.0, sprint: 1.2, ..avoid_entity::Params::DEFAULT });
pub const ATTACK: Entry = Entry::new(&melee::MODULE, 4, &melee::Params { follow_unseen: true, quit_in_light: Some(100), ..melee::Params::DEFAULT }).when(&NO_RIDER);

pub static DEF: Def = Def {
    name: "minecraft:spider",
    targets: TARGETS,
    goals: &[
        Entry::new(&float::MODULE, 1, &float::Params::DEFAULT),
        ARMADILLO,
        Entry::new(&leap_at_target::MODULE, 3, &leap_at_target::Params::DEFAULT),
        ATTACK,
        Entry::new(&stroll::MODULE, 5, &stroll::Params { speed: 0.8, ..stroll::Params::DEFAULT }),
        Entry::new(&look_at_player::MODULE, 6, &look_at_player::Params { range: 8.0, ..look_at_player::Params::DEFAULT }),
        Entry::new(&random_look::MODULE, 6, &random_look::Params::DEFAULT),
    ],
    locomotion: &[(Medium::Ground, Entry::new(&walk::MODULE, 0, &walk::Params { climber: true }))],
    rules: &[RuleEntry::new(&RIDE, &RideParams::CARRIER)],
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Head, Cap::Targets, Cap::Locomotion, Cap::Rideable]),
    spawn: Some(crate::spawning::SpawnDef::MONSTER), despawn: Some(crate::despawn::DespawnDef::MOB),
    ..Def::EMPTY
};
