//! `Creeper` (`mobs/hostile.rs:18-39`): its targets, then its goals. Left out: the two avoidances of the cat and of the ocelot
//! (`avoid_entity` names the species by identifier; those species are not in the registry). The rule `R:creeper` is not ported.
use crate::behaviours::avoid_entity::{self, Avoid};
use crate::behaviours::{float, hurt_by, look_at_player, melee, nearest_target, random_look, stroll, swell, Entry};
use crate::species::Def;

static AVOID_OCELOT: avoid_entity::Params = avoid_entity::Params { what: Avoid::Named(&["minecraft:ocelot"]), max_dist: 6.0, walk: 1.0, sprint: 1.2, ..avoid_entity::Params::DEFAULT };
static AVOID_CAT: avoid_entity::Params = avoid_entity::Params { what: Avoid::Named(&["minecraft:cat"]), max_dist: 6.0, walk: 1.0, sprint: 1.2, ..avoid_entity::Params::DEFAULT };

pub static DEF: Def = Def {
    name: "minecraft:creeper",
    targets: &[Entry::new(&nearest_target::MODULE, 1, &nearest_target::Params::PLAYER), Entry::new(&hurt_by::MODULE, 2, &hurt_by::Params::DEFAULT)],
    goals: &[
        Entry::new(&float::MODULE, 1, &float::Params::DEFAULT),
        Entry::new(&swell::MODULE, 2, &swell::Params::DEFAULT),
        Entry::new(&avoid_entity::MODULE, 3, &AVOID_OCELOT),
        Entry::new(&avoid_entity::MODULE, 3, &AVOID_CAT),
        Entry::new(&melee::MODULE, 4, &melee::Params::DEFAULT),
        Entry::new(&stroll::MODULE, 5, &stroll::Params { speed: 0.8, ..stroll::Params::DEFAULT }),
        Entry::new(&look_at_player::MODULE, 6, &look_at_player::Params { range: 8.0, ..look_at_player::Params::DEFAULT }),
        Entry::new(&random_look::MODULE, 6, &random_look::Params::DEFAULT),
    ],
    spawn: Some(crate::spawning::SpawnDef::MONSTER), despawn: Some(crate::despawn::DespawnDef::MOB),
    not_in_peaceful: true,
    rules: &[crate::rules::RuleEntry::new(&crate::loot::XP, &crate::loot::XpReward { min: 5, max: 5 }), 
        crate::rules::RuleEntry::new(&crate::ignite::IGNITE, &crate::ignite::IgniteParams::CREEPER),
        crate::rules::RuleEntry::new(&crate::fall_by_health::FALL_BY_HEALTH, &()),
    ],
    sound_source: super::SoundCategory::Hostile,
    ..Def::EMPTY
};
