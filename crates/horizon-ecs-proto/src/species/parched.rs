//! `Parched` (`mobs/undead.rs:148-188`): the skeleton's goals and targets, its bow on normal every 70 ticks (`undead.rs:183-188`).
//! It does not burn in daylight (`burns_in_day` omits it), so it refers to the monster base, not to the skeleton, whose rule
//! `sun_burn` cannot be removed by a change. Left out: the weakness its arrows carry (`undead.rs:952`): `attend : effet de flèche`.
//! `attend : flee_sun, avoid_entity (loup)` : the skeleton's pending goals, to add here when the skeleton has them.
use super::{templates::monster, Change, Def, Section};
use crate::behaviours::{nearest_target, ranged_bow, restrict_sun, Entry};
use crate::effects::Kind;

pub static DEF: Def = Def {
    name: "minecraft:parched",
    reference: Some(&monster::DEF),
    changes: &[
        Change::Add(Section::Targets, Entry::new(&nearest_target::MODULE, 3, &nearest_target::Params { types: &["minecraft:iron_golem"], ..nearest_target::Params::DEFAULT })),
        Change::Add(Section::Targets, Entry::new(&nearest_target::MODULE, 3, &nearest_target::Params { types: &["minecraft:turtle"], predicate: Some("baby"), interval: 10, ..nearest_target::Params::DEFAULT })),
        Change::Add(Section::Goals, Entry::new(&restrict_sun::MODULE, 2, &restrict_sun::Params)),
        Change::Add(Section::Goals, Entry::new(&ranged_bow::MODULE, 4, &ranged_bow::Params { interval: 70, effect: Some((Kind::Weakness, 0, 600)), ..ranged_bow::Params::DEFAULT })),
    ],
    ..Def::EMPTY
};
