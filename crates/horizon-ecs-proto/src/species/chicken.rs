//! `Chicken.registerGoals`: the base with a faster panic and seeds (the egg timer is a rule of `Chicken.aiStep`).
use super::{templates::farm_animal, Change, Def};
use crate::behaviours::{follow_parent, panic, tempt};

pub static DEF: Def = Def {
    name: "minecraft:chicken",
    reference: Some(&farm_animal::DEF),
    changes: &[
        Change::Set { module: &panic::MODULE, nth: 0, params: &panic::Params { speed: 1.4, ..panic::Params::DEFAULT } },
        Change::Set {
            module: &tempt::MODULE,
            nth: 0,
            params: &tempt::Params { speed: 1.0, items: &["minecraft:wheat_seeds", "minecraft:beetroot_seeds", "minecraft:melon_seeds", "minecraft:pumpkin_seeds"], ..tempt::Params::DEFAULT },
        },
        Change::Set { module: &follow_parent::MODULE, nth: 0, params: &follow_parent::Params { speed: 1.1 } },
    ],
    ..Def::EMPTY
};
