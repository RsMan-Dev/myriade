//! `Chicken.registerGoals`: the base with a faster panic and seeds (the egg timer is a rule of `Chicken.aiStep`).
use super::{templates::farm_animal, Change, Def};
use crate::rules::RuleEntry;
use crate::behaviours::{breed, follow_parent, panic, tempt, Cap};
use crate::mount::{RideParams, RIDE};

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
        Change::Set { module: &breed::MODULE, nth: 0, params: &breed::Params { foods: &["minecraft:wheat_seeds", "minecraft:beetroot_seeds", "minecraft:melon_seeds", "minecraft:pumpkin_seeds"], ..breed::Params::DEFAULT } },
        Change::Set { module: &follow_parent::MODULE, nth: 0, params: &follow_parent::Params { speed: 1.1 } },
    ],
    rules: &[
        RuleEntry::new(&crate::farm::LAY_EGG, &crate::farm::EggDef { base: 6000, range: 6000 }),
        RuleEntry::new(&crate::farm::WING_FLAP, &crate::farm::WingParams { fall_factor: 0.6, fall_immune: true }),
        RuleEntry::new(&RIDE, &RideParams::CARRIER),
    ],
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Head, Cap::Targets, Cap::Locomotion, Cap::Rideable]),
    ..Def::EMPTY
};
