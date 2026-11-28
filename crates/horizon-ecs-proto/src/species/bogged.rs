//! `Bogged` (`mobs/undead.rs:148-188`): the skeleton's goals, its bow on normal every 70 ticks (`BowAttack` interval 70, the hard one 50).
//! Left out: the poison its arrows carry (`perform_ranged_attack`, `undead.rs:951`): `attend : effet de flèche`.
use super::{skeleton, Change, Def};
use crate::behaviours::{ranged_bow, ambient_sound, Entry};
use crate::behaviours::shear::ShearDef;
use crate::dispense::Sheared;
use crate::effects::Kind;

/// `Bogged.shear`: the mushrooms fall from its height and it stays sheared (saved as `sheared`).
pub static SHEARING: ShearDef = ShearDef {
    equip: |_, _| {},
    ready: |w, e| w.get::<Sheared>(e).is_none(),
    apply: |w, e| {
        w.entity_mut(e).insert(Sheared);
        true
    },
    table: |_, _| Some("minecraft:shearing/bogged".to_owned()),
    sound: "minecraft:entity.bogged.shear",
    height: None,
    puff: None,
};

pub static DEF: Def = Def {
    name: "minecraft:bogged",
    reference: Some(&skeleton::DEF),
    changes: &[Change::Set { module: &ranged_bow::MODULE, nth: 0, params: &ranged_bow::Params { interval: 70, effect: Some((Kind::Poison, 0, 100)), ..ranged_bow::Params::DEFAULT } }, Change::Set { module: &ambient_sound::MODULE, nth: 0, params: &ambient_sound::Params { sounds: &["entity.bogged.ambient"], interval: Some(80), ..ambient_sound::Params::DEFAULT } }],
    rules: &[crate::rules::RuleEntry::new(&crate::behaviours::shear::SHEAR, &SHEARING)],
    not_in_peaceful: true,
    sound_source: super::SoundCategory::Hostile,
    ..Def::EMPTY
};
