//! `Sheep.registerGoals`: the base with slower panic and tempt, and grass eating before the stroll.
use super::{templates::farm_animal, Change, Def};
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
    rules: &[RuleEntry::new(&crate::farm::WOOL, &())],
    ..Def::EMPTY
};
