//! `AbstractSkeleton.registerGoals` (`mobs/undead.rs:148-182`): the monster base, the bow goal at priority 4 (the bow in hand is supposed;
//! the normal-difficulty interval of 40 ticks), the flight from the sun (3), the flight from wolves (`AvoidEntity` 6 / 1,0 / 1,2), and the
//! targets of the iron golem and of the baby turtle (3, seen, every 10 ticks). Left out: `RestrictSun` (2: `Nav.avoid_sun`, absent).
//! The stray, bogged, parched and wither skeletons stay out of this lot (their files do not exist).
use super::{templates::monster, Change, Def, Section};
use crate::behaviours::{avoid_entity, flee_sun, nearest_target, ranged_bow, restrict_sun, Entry};
use crate::rules::RuleEntry;

pub static DEF: Def = Def {
    name: "minecraft:skeleton",
    reference: Some(&monster::DEF),
    changes: &[
        Change::Add(Section::Goals, Entry::new(&restrict_sun::MODULE, 2, &restrict_sun::Params)),
        Change::Add(Section::Goals, Entry::new(&ranged_bow::MODULE, 4, &ranged_bow::Params { interval: 40, ..ranged_bow::Params::DEFAULT })),
        Change::Add(Section::Goals, Entry::new(&crate::behaviours::pick_up_gear::MODULE, 255, &crate::behaviours::pick_up_gear::Params { refuses: &["#minecraft:spears"], preferred_weapons: Some("minecraft:skeleton_preferred_weapons"), ..crate::behaviours::pick_up_gear::Params::DEFAULT })),
        Change::Add(Section::Goals, Entry::new(&flee_sun::MODULE, 3, &flee_sun::Params { speed: 1.0 })),
        Change::Add(
            Section::Goals,
            Entry::new(&avoid_entity::MODULE, 3, &avoid_entity::Params { what: avoid_entity::Avoid::Named(&["minecraft:wolf"]), max_dist: 6.0, walk: 1.0, sprint: 1.2, ..avoid_entity::Params::DEFAULT }),
        ),
        Change::Add(Section::Targets, Entry::new(&nearest_target::MODULE, 3, &nearest_target::Params { types: &["minecraft:iron_golem"], must_see: true, ..nearest_target::Params::DEFAULT })),
        Change::Add(
            Section::Targets,
            Entry::new(&nearest_target::MODULE, 3, &nearest_target::Params { types: &["minecraft:turtle"], predicate: Some("baby"), must_see: true, interval: 10, ..nearest_target::Params::DEFAULT }),
        ),
    ],
    caps: Some(super::FREEZE_IMMUNE_CAPS),
    rules: &[RuleEntry::new(&crate::hostile::SUN_BURN, &()), RuleEntry::new(&crate::spawn_gear::RULE, &crate::spawn_gear::SKELETON)],
    not_in_peaceful: true,
    sound_source: super::SoundCategory::Hostile,
    ..Def::EMPTY
};
