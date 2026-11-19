//! `CamelHusk` (`equine.rs:204-205`, `camel_husk_tests.rs:10-12`): the camel without breeding; any rider holds its panic back.
use super::{camel, Change, Def};
use crate::behaviours::{animal_panic, breed};

pub static DEF: Def = Def {
    name: "minecraft:camel_husk",
    reference: Some(&camel::DEF),
    changes: &[
        Change::Remove { module: &breed::MODULE, nth: 0 },
        Change::Set { module: &animal_panic::MODULE, nth: 0, params: &animal_panic::Params { speed: 4.0, riders: animal_panic::Riders::Any, stand_up: true, ..animal_panic::Params::DEFAULT } },
    ],
    ..Def::EMPTY
};
