//! `cave_spider` (`mobs/hostile.rs:42-60`): the walk of a WALL CLIMBER (`walk::Params.climber`: the wall is climbed, the climb goes on straight at the target
//! when the path is over). Goals: float (1), leap at the target (3), melee that follows unseen (4), stroll (5), look (6); targets: hurt by (1),
//! the nearest player (2). Left out: the armadillo avoidance (not in the registry), the darkness gate of the targets and of the attack, the
//! iron golem target.
use super::Def;
use crate::behaviours::{float, hurt_by, leap_at_target, look_at_player, melee, nearest_target, random_look, stroll, Entry};
use crate::locomotion::{walk, Medium};

pub static DEF: Def = Def {
    name: "minecraft:cave_spider",
    targets: &[Entry::new(&hurt_by::MODULE, 1, &hurt_by::Params::DEFAULT), Entry::new(&nearest_target::MODULE, 2, &nearest_target::Params::PLAYER)],
    goals: &[
        Entry::new(&float::MODULE, 1, &float::Params::DEFAULT),
        Entry::new(&leap_at_target::MODULE, 3, &leap_at_target::Params::DEFAULT),
        Entry::new(&melee::MODULE, 4, &melee::Params { follow_unseen: true, ..melee::Params::DEFAULT }),
        Entry::new(&stroll::MODULE, 5, &stroll::Params { speed: 0.8, ..stroll::Params::DEFAULT }),
        Entry::new(&look_at_player::MODULE, 6, &look_at_player::Params { range: 8.0, ..look_at_player::Params::DEFAULT }),
        Entry::new(&random_look::MODULE, 6, &random_look::Params::DEFAULT),
    ],
    locomotion: &[(Medium::Ground, Entry::new(&walk::MODULE, 0, &walk::Params { climber: true }))],
        rules: &[crate::rules::RuleEntry::new(&crate::effects::ON_HIT, &crate::effects::CAVE_SPIDER)],
    ..Def::EMPTY
};
