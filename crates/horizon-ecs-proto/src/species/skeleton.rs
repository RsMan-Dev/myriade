//! `AbstractSkeleton.registerGoals`: the monster base, and the bow goal at priority 4 (the bow in hand is supposed; the
//! normal-difficulty interval of 40 ticks; the sun goals and the wolf avoidance are not ported yet).
use super::{templates::monster, Change, Def, Section};
use crate::behaviours::{ranged_bow, Entry};

pub static DEF: Def = Def {
    name: "minecraft:skeleton",
    reference: Some(&monster::DEF),
    changes: &[Change::Add(Section::Goals, Entry::new(&ranged_bow::MODULE, 4, &ranged_bow::Params { interval: 40, ..ranged_bow::Params::DEFAULT }))],
    ..Def::EMPTY
};
