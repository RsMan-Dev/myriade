//! `Evoker` (`illager.rs:109-130`): float (0), `spell_casting` (1), the avoidance of the player (2, 8 blocks, 0.6 / 1.0) and of the creaking (3), the vexes
//! (4) and the fangs (5), the stroll at 0.6 (8), looking (9, 10); targets: hurt-by with the alert (1), the nearest player (2), the villagers and
//! wandering traders unseen allowed (3), the iron golem (3), all with 300 ticks of memory. Left out: the wololo spell (6: sheep colours),
//! the raid goals (`pathfind_to_raid`, `HoldGroundAttack`: the village and raid porter), the raiders the hurt-by ignores, the `look_at_player` of mobs (10).
use super::Def;
use crate::behaviours::{avoid_entity, float, hurt_by, look_at_player, nearest_target, spell_casting, spell_use, stroll, Entry, ambient_sound};

/// `HurtByTargetGoal(this, Raider.class)`-style list of the illagers (jar and Mob): the attackers they never answer.
pub const RAIDERS: &[&str] = &["minecraft:pillager", "minecraft:vindicator", "minecraft:witch", "minecraft:evoker", "minecraft:illusioner", "minecraft:vex", "minecraft:ravager"];
const TRADERS: &[&str] = &["minecraft:villager", "minecraft:wandering_trader"];
pub const VEXES: spell_use::Params = spell_use::Params { spell: spell_use::Spell::Vexes { species: "minecraft:vex" }, id: 1, warmup: 20, casting: 100, interval: 340 };
pub const FANGS: spell_use::Params = spell_use::Params { spell: spell_use::Spell::Fangs { allies: &["minecraft:evoker", "minecraft:vex", "minecraft:pillager", "minecraft:vindicator", "minecraft:illusioner", "minecraft:ravager", "minecraft:witch"] }, id: 2, warmup: 20, casting: 40, interval: 100 };
pub const TARGETS: &[Entry] = &[
    Entry::new(&hurt_by::MODULE, 1, &hurt_by::Params { alert: true, ignoring: RAIDERS, ..hurt_by::Params::DEFAULT }),
    Entry::new(&nearest_target::MODULE, 2, &nearest_target::Params { unseen_memory: 300, ..nearest_target::Params::PLAYER }),
    Entry::new(&nearest_target::MODULE, 3, &nearest_target::Params { types: TRADERS, must_see: false, unseen_memory: 300, ..nearest_target::Params::DEFAULT }),
    Entry::new(&nearest_target::MODULE, 3, &nearest_target::Params { types: &["minecraft:iron_golem"], must_see: false, ..nearest_target::Params::DEFAULT }),
];
pub static DEF: Def = Def {
    rules: &[crate::rules::RuleEntry::new(&crate::loot::XP, &crate::loot::XpReward { min: 10, max: 10 })],
    name: "minecraft:evoker",
    targets: TARGETS,
    goals: &[
        Entry::new(&crate::behaviours::obtain_banner::MODULE, 1, &crate::behaviours::obtain_banner::Params::DEFAULT),
        Entry::new(&crate::behaviours::raid_move_through_village::MODULE, 4, &crate::behaviours::raid_move_through_village::Params::DEFAULT),
        Entry::new(&crate::behaviours::long_distance_patrol::MODULE, 4, &crate::behaviours::long_distance_patrol::Params::DEFAULT),
        Entry::new(&crate::behaviours::celebration::MODULE, 5, &()),
        Entry::new(&crate::behaviours::pathfind_to_raid::MODULE, 3, &crate::behaviours::pathfind_to_raid::Params::DEFAULT),
        Entry::new(&float::MODULE, 0, &float::Params::DEFAULT),
        Entry::new(&spell_casting::MODULE, 1, &()),
        Entry::new(&avoid_entity::MODULE, 2, &avoid_entity::Params { what: avoid_entity::Avoid::Player, max_dist: 8.0, walk: 0.6, sprint: 1.0, ..avoid_entity::Params::DEFAULT }),
        Entry::new(&avoid_entity::MODULE, 3, &avoid_entity::Params { what: avoid_entity::Avoid::Named(&["minecraft:creaking"]), max_dist: 8.0, walk: 0.6, sprint: 1.0, ..avoid_entity::Params::DEFAULT }),
        Entry::new(&spell_use::MODULE, 4, &VEXES),
        Entry::new(&spell_use::MODULE, 5, &FANGS),
        Entry::new(&spell_use::MODULE, 6, &spell_use::Params::WOLOLO),
        Entry::new(&stroll::MODULE, 8, &stroll::Params { speed: 0.6, ..stroll::Params::DEFAULT }),
        Entry::new(&look_at_player::MODULE, 9, &look_at_player::Params { range: 3.0, period: 1, ..look_at_player::Params::DEFAULT }),
        Entry::new(&look_at_player::MODULE, 10, &look_at_player::Params { range: 8.0, species: &["*"], ..look_at_player::Params::DEFAULT }),
        Entry::new(&ambient_sound::MODULE, 14, &ambient_sound::Params { sounds: &["entity.evoker.ambient"], interval: Some(80), ..ambient_sound::Params::DEFAULT }),
    ],
    not_in_peaceful: true,
    sound_source: super::SoundCategory::Hostile,
    ..Def::EMPTY
};
