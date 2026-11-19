//! `Pig.registerGoals`: two tempt goals of one priority (the carrot on a stick, then its foods), each with its
//! calm-down; the others a priority later.
use super::{templates::farm_animal, Change, Def};
use crate::behaviours::{breed, follow_parent, look_at_player, panic, random_look, stroll, tempt, Cap, Entry};
use crate::mount::{RideParams, RIDE};
use crate::rules::RuleEntry;

pub static DEF: Def = Def {
    name: "minecraft:pig",
    reference: Some(&farm_animal::DEF),
    changes: &[
        Change::Set { module: &panic::MODULE, nth: 0, params: &panic::Params { speed: 1.25, ..panic::Params::DEFAULT } },
        Change::Set { module: &breed::MODULE, nth: 0, params: &breed::Params { foods: &["minecraft:carrot", "minecraft:potato", "minecraft:beetroot"], ..breed::Params::DEFAULT } },
        Change::Priority { module: &breed::MODULE, nth: 0, priority: 3 },
        Change::Set { module: &tempt::MODULE, nth: 0, params: &tempt::Params { speed: 1.2, items: &["minecraft:carrot_on_a_stick"], ..tempt::Params::DEFAULT } },
        Change::Priority { module: &tempt::MODULE, nth: 0, priority: 4 },
        Change::InsertAfter {
            module: &tempt::MODULE,
            nth: 0,
            entry: Entry::new(&tempt::MODULE, 4, &tempt::Params { speed: 1.2, items: &["minecraft:carrot", "minecraft:potato", "minecraft:beetroot"], ..tempt::Params::DEFAULT }),
        },
        Change::Set { module: &follow_parent::MODULE, nth: 0, params: &follow_parent::Params { speed: 1.1 } },
        Change::Priority { module: &follow_parent::MODULE, nth: 0, priority: 5 },
        Change::Priority { module: &stroll::MODULE, nth: 0, priority: 6 },
        Change::Priority { module: &look_at_player::MODULE, nth: 0, priority: 7 },
        Change::Priority { module: &random_look::MODULE, nth: 0, priority: 8 },
    ],
    rules: &[RuleEntry::new(&RIDE, &RideParams::PIG), RuleEntry::new(&crate::leash::LEASH, &crate::leash::LeashParams::DEFAULT), RuleEntry::new(&crate::equipment::EQUIP, &crate::equipment::EquipParams::SADDLE)],
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Head, Cap::Targets, Cap::Locomotion, Cap::Rideable]),
    ..Def::EMPTY
};
