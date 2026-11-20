//! `Illusioner` (`illusioner.rs:31-62`): float (0), `spell_casting` (1), the avoidance of the creaking (3), the mirror spell (4: invisible for 60 s), the bow
//! (6, speed 0.5, every 20 ticks, 15 blocks), the stroll at 0.6 (8), looking (9); targets as the evoker's. Left out: the blindness spell (5: no
//! blindness effect), the mirror images (a client drawing), the raid goals, the creaking-free `look_at_player` of mobs.
use super::Def;
use crate::behaviours::{avoid_entity, celebration, float, hurt_by, long_distance_patrol, look_at_player, nearest_target, obtain_banner, pathfind_to_raid, raid_move_through_village, ranged_bow, spell_casting, spell_use, stroll, Entry};

const TRADERS: &[&str] = &["minecraft:villager", "minecraft:wandering_trader"];
/// All three nearest-target entries keep 300 ticks of memory; the villagers and the golem are taken unseen (jar).
pub const TARGETS: &[Entry] = &[
    Entry::new(&hurt_by::MODULE, 1, &hurt_by::Params { alert: true, ..hurt_by::Params::DEFAULT }),
    Entry::new(&nearest_target::MODULE, 2, &nearest_target::Params { unseen_memory: 300, ..nearest_target::Params::PLAYER }),
    Entry::new(&nearest_target::MODULE, 3, &nearest_target::Params { types: TRADERS, must_see: false, unseen_memory: 300, ..nearest_target::Params::DEFAULT }),
    Entry::new(&nearest_target::MODULE, 3, &nearest_target::Params { types: &["minecraft:iron_golem"], must_see: false, unseen_memory: 300, ..nearest_target::Params::DEFAULT }),
];

pub static DEF: Def = Def {
    name: "minecraft:illusioner",
    targets: TARGETS,
    goals: &[
        Entry::new(&obtain_banner::MODULE, 1, &obtain_banner::Params::DEFAULT),
        Entry::new(&raid_move_through_village::MODULE, 4, &raid_move_through_village::Params::DEFAULT),
        Entry::new(&long_distance_patrol::MODULE, 4, &long_distance_patrol::Params::DEFAULT),
        Entry::new(&celebration::MODULE, 5, &()),
        Entry::new(&pathfind_to_raid::MODULE, 3, &pathfind_to_raid::Params::DEFAULT),
        Entry::new(&float::MODULE, 0, &float::Params::DEFAULT),
        Entry::new(&spell_casting::MODULE, 1, &()),
        Entry::new(&avoid_entity::MODULE, 3, &avoid_entity::Params { what: avoid_entity::Avoid::Named(&["minecraft:creaking"]), max_dist: 8.0, walk: 1.0, sprint: 1.2, ..avoid_entity::Params::DEFAULT }),
        Entry::new(&spell_use::MODULE, 4, &spell_use::Params::MIRROR),
        Entry::new(&spell_use::MODULE, 5, &spell_use::Params::BLINDNESS),
        Entry::new(&ranged_bow::MODULE, 6, &ranged_bow::Params { speed: 0.5, interval: 20, radius: 15.0, ..ranged_bow::Params::DEFAULT }),
        Entry::new(&stroll::MODULE, 8, &stroll::Params { speed: 0.6, ..stroll::Params::DEFAULT }),
        Entry::new(&look_at_player::MODULE, 9, &look_at_player::Params { range: 3.0, period: 1, ..look_at_player::Params::DEFAULT }),
        Entry::new(&look_at_player::MODULE, 10, &look_at_player::Params { range: 8.0, species: &["*"], ..look_at_player::Params::DEFAULT }),
    ],
    ..Def::EMPTY
};
