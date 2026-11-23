//! `SnowGolem` (`golem.rs:548-565`): a ranged attack with snowballs, looking at players and random looks. Left out, waiting for their
//! predicate, parameter or module: the hostile target (`Prey::Filter(hostile)`), the stroll with its water probability (`stroll` has no
//! such parameter), (the snow trail is in, at 255: no flag).
use super::Def;
use crate::behaviours::shear::ShearDef;
use crate::behaviours::{look_at_player, nearest_target, random_look, ranged_attack, snow_trail, stroll, Entry};
use bevy_ecs::prelude::*;

/// `SnowGolem.hasPumpkin`: present on every snow golem, true by default (the NBT byte `Pumpkin`).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Pumpkin(pub bool);

/// `SnowGolem.shear`: the pumpkin comes off and falls from 1.7 blocks up.
pub static SHEARING: ShearDef = ShearDef {
    equip: |w, e| {
        w.entity_mut(e).insert(Pumpkin(true));
    },
    ready: |w, e| w.get::<Pumpkin>(e).is_some_and(|p| p.0),
    apply: |w, e| {
        w.get_mut::<Pumpkin>(e).is_some_and(|mut p| {
            p.0 = false;
            true
        })
    },
    table: |_, _| Some("minecraft:shearing/snow_golem".to_owned()),
    sound: "minecraft:entity.snow_golem.shear",
    height: Some(1.7),
    puff: None,
};

pub static DEF: Def = Def {
    name: "minecraft:snow_golem",
    reference: None,
    targets: &[Entry::new(&nearest_target::MODULE, 1, &nearest_target::Params { hostile: true, ..nearest_target::Params::DEFAULT })],
    goals: &[
        Entry::new(&ranged_attack::MODULE, 1, &ranged_attack::Params::DEFAULT),
        Entry::new(&stroll::MODULE, 2, &stroll::Params { speed: 1.0, interval: None, water_avoiding: 1.000_000_1e-5 }),
        Entry::new(&snow_trail::MODULE, 255, &snow_trail::Params::DEFAULT),
        Entry::new(&look_at_player::MODULE, 3, &look_at_player::Params { range: 6.0, ..look_at_player::Params::DEFAULT }),
        Entry::new(&random_look::MODULE, 4, &random_look::Params::DEFAULT),
    ],
    caps: Some(super::FREEZE_IMMUNE_CAPS),
    rules: &[crate::rules::RuleEntry::new(&crate::behaviours::shear::SHEAR, &SHEARING)],
    ..Def::EMPTY
};
