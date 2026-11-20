//! `Illusioner` (`illusioner.rs:31-62`): float (0), `spell_casting` (1), the avoidance of the creaking (3), the mirror spell (4: invisible for 60 s), the bow
//! (6, speed 0.5, every 20 ticks, 15 blocks), the stroll at 0.6 (8), looking (9); targets as the evoker's. Left out: the blindness spell (5: no
//! blindness effect), the mirror images (a client drawing), the raid goals, the creaking-free `look_at_player` of mobs.
use super::Def;
use crate::behaviours::{avoid_entity, float, look_at_player, ranged_bow, spell_casting, spell_use, stroll, Entry};

pub static DEF: Def = Def {
    name: "minecraft:illusioner",
    targets: super::evoker::TARGETS,
    goals: &[
        Entry::new(&float::MODULE, 0, &float::Params::DEFAULT),
        Entry::new(&spell_casting::MODULE, 1, &()),
        Entry::new(&avoid_entity::MODULE, 3, &avoid_entity::Params { what: avoid_entity::Avoid::Named(&["minecraft:creaking"]), max_dist: 8.0, walk: 1.0, sprint: 1.2, ..avoid_entity::Params::DEFAULT }),
        Entry::new(&spell_use::MODULE, 4, &spell_use::Params::MIRROR),
        Entry::new(&ranged_bow::MODULE, 6, &ranged_bow::Params { speed: 0.5, interval: 20, radius: 15.0, ..ranged_bow::Params::DEFAULT }),
        Entry::new(&stroll::MODULE, 8, &stroll::Params { speed: 0.6, ..stroll::Params::DEFAULT }),
        Entry::new(&look_at_player::MODULE, 9, &look_at_player::Params { range: 3.0, period: 1 }),
    ],
    ..Def::EMPTY
};
