//! `Parched` (`mobs/undead.rs:148-188`): the skeleton's goals and targets, its bow on normal every 70 ticks (`undead.rs:183-188`).
//! It does not burn in daylight (`burns_in_day` omits it), so it refers to the monster base, not to the skeleton, whose rule
//! `sun_burn` cannot be removed by a change. Left out: the weakness its arrows carry (`undead.rs:952`): `attend : effet de flèche`.
//! It flees the sun and avoids the wolf as the skeleton does.
use super::{templates::monster, Change, Def, Section};
use crate::behaviours::{avoid_entity, flee_sun, nearest_target, ranged_bow, restrict_sun, Entry};
use crate::effects::Kind;

pub static DEF: Def = Def {
    name: "minecraft:parched",
    caps: Some(super::WEAKNESS_IMMUNE_CAPS),
    reference: Some(&monster::DEF),
    changes: &[
        Change::Add(Section::Targets, Entry::new(&nearest_target::MODULE, 3, &nearest_target::Params { types: &["minecraft:iron_golem"], ..nearest_target::Params::DEFAULT })),
        Change::Add(Section::Targets, Entry::new(&nearest_target::MODULE, 3, &nearest_target::Params { types: &["minecraft:turtle"], predicate: Some("baby"), interval: 10, ..nearest_target::Params::DEFAULT })),
        Change::Add(Section::Goals, Entry::new(&restrict_sun::MODULE, 2, &restrict_sun::Params)),
        Change::Add(Section::Goals, Entry::new(&flee_sun::MODULE, 3, &flee_sun::Params { speed: 1.0 })),
        Change::Add(
            Section::Goals,
            Entry::new(&avoid_entity::MODULE, 3, &avoid_entity::Params { what: avoid_entity::Avoid::Named(&["minecraft:wolf"]), max_dist: 6.0, walk: 1.0, sprint: 1.2, ..avoid_entity::Params::DEFAULT }),
        ),
        Change::Add(Section::Goals, Entry::new(&ranged_bow::MODULE, 4, &ranged_bow::Params { interval: 70, effect: Some((Kind::Weakness, 0, 600)), ..ranged_bow::Params::DEFAULT })),
    ],
    ..Def::EMPTY
};
