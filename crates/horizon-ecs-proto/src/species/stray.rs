//! `Stray` (`mobs/undead.rs:148-188`): the skeleton's goals and bow, the bow's interval on normal being the skeleton's (40). Left out:
//! the slowness its arrows carry (`perform_ranged_attack`, `undead.rs:950`), an effect of the arrow: `attend : effet de flèche`.
use super::{skeleton, Change, Def};
use crate::behaviours::ranged_bow;
use crate::effects::Kind;

pub static DEF: Def = Def {
    name: "minecraft:stray",
    reference: Some(&skeleton::DEF),
    // `undead.rs:950`: its arrows carry slowness (level 0, 600 ticks); the bow's interval is the skeleton's (40).
    changes: &[Change::Set { module: &ranged_bow::MODULE, nth: 0, params: &ranged_bow::Params { interval: 40, effect: Some((Kind::Slowness, 0, 600)), ..ranged_bow::Params::DEFAULT } }],
    not_in_peaceful: true,
    sound_source: super::SoundCategory::Hostile,
    ..Def::EMPTY
};
