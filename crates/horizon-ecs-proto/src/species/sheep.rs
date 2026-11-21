//! `Sheep.registerGoals`: the base with slower panic and tempt, and grass eating before the stroll.
use super::{templates::farm_animal, Change, Def};
use crate::behaviours::shear::{adult, ShearDef};
use crate::farm::Wool;
use crate::rules::RuleEntry;
use crate::behaviours::{eat_block, follow_parent, look_at_player, panic, random_look, stroll, tempt, Entry};

pub static DEF: Def = Def {
    name: "minecraft:sheep",
    reference: Some(&farm_animal::DEF),
    changes: &[
        Change::Set { module: &panic::MODULE, nth: 0, params: &panic::Params { speed: 1.25, ..panic::Params::DEFAULT } },
        Change::Set { module: &tempt::MODULE, nth: 0, params: &tempt::Params { speed: 1.1, items: &["minecraft:wheat"], ..tempt::Params::DEFAULT } },
        Change::Set { module: &follow_parent::MODULE, nth: 0, params: &follow_parent::Params { speed: 1.1 } },
        Change::InsertAfter { module: &follow_parent::MODULE, nth: 0, entry: Entry::new(&eat_block::MODULE, 5, &eat_block::Params::DEFAULT) },
        Change::Priority { module: &stroll::MODULE, nth: 0, priority: 6 },
        Change::Priority { module: &look_at_player::MODULE, nth: 0, priority: 7 },
        Change::Priority { module: &random_look::MODULE, nth: 0, priority: 8 },
    ],
    rules: &[RuleEntry::new(&crate::behaviours::shear::SHEAR, &SHEARING), RuleEntry::new(&crate::dye::DYE, &crate::dye::DyeParams::SHEEP)],
    draws: Some(&DRAWS),
    ..Def::EMPTY
};

/// `SheepColorSpawnRules.getSheepColor`: the climate of the biome's tags (cold, warm, else temperate) picks a list of weights out of 100
/// (dye ids: white 0, pink 6, gray 7, light gray 8, brown 12, black 15); past it the climate's common colour, pink 1 time in 500.
use crate::rules::spawn_draw::Roll;
static TEMPERATE: Roll = Roll::Weighted { total: 100, list: &[(15, 5), (7, 5), (8, 5), (12, 3)], rest: &Roll::Cuts { bound: 500, cuts: &[(499, 0)], last: 6 } };
static WARM: Roll = Roll::Weighted { total: 100, list: &[(7, 5), (8, 5), (0, 5), (15, 3)], rest: &Roll::Cuts { bound: 500, cuts: &[(499, 12)], last: 6 } };
static COLD: Roll = Roll::Weighted { total: 100, list: &[(8, 5), (7, 5), (0, 5), (12, 3)], rest: &Roll::Cuts { bound: 500, cuts: &[(499, 15)], last: 6 } };
static DRAWS: crate::behaviours::spawn_draw::Draws = crate::behaviours::spawn_draw::Draws {
    group: None,
    list: &[crate::behaviours::spawn_draw::Draw {
        when: crate::behaviours::spawn_draw::When::Before,
        kind: crate::behaviours::spawn_draw::Kind::Plain(crate::rules::spawn_draw::Roll::ByBiome {
            rows: &[("#minecraft:spawns_cold_variant_farm_animals", &COLD), ("#minecraft:spawns_warm_variant_farm_animals", &WARM)],
            otherwise: &TEMPERATE,
        }),
        store: crate::behaviours::spawn_draw::Store::WoolColor,
    }],
};

/// `Sheep.shear`: the wool falls by its colour and the sheep stays bare until it eats grass again (`eat_block`).
pub static SHEARING: ShearDef = ShearDef {
    equip: |w, e| {
        w.entity_mut(e).insert(Wool::default());
    },
    ready: |w, e| adult(w, e) && w.get::<Wool>(e).is_some_and(|x| !x.sheared),
    apply: |w, e| {
        w.get_mut::<Wool>(e).is_some_and(|mut x| {
            x.sheared = true;
            true
        })
    },
    table: |w, e| w.get::<Wool>(e).map(|x| format!("minecraft:shearing/sheep/{}", super::cushion::DYES[usize::from(x.color & 15)])),
    sound: "minecraft:entity.sheep.shear",
    height: Some(1.0),
    puff: None,
};
