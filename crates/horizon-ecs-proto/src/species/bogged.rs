//! `Bogged` (`mobs/undead.rs:148-188`): the skeleton's goals, its bow on normal every 70 ticks (`BowAttack` interval 70, the hard one 50).
//! Left out: the poison its arrows carry (`perform_ranged_attack`, `undead.rs:951`): `attend : effet de flèche`.
use super::{skeleton, Change, Def};
use crate::behaviours::ranged_bow;
use crate::effects::Kind;

pub static DEF: Def = Def {
    name: "minecraft:bogged",
    reference: Some(&skeleton::DEF),
    changes: &[Change::Set { module: &ranged_bow::MODULE, nth: 0, params: &ranged_bow::Params { interval: 70, effect: Some((Kind::Poison, 0, 100)), ..ranged_bow::Params::DEFAULT } }],
    ..Def::EMPTY
};
