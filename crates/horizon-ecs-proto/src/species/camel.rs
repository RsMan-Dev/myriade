//! `Camel` (`horizon-client/src/mobs/camel.rs:26-45`): the goals of the camel in the `Mob`'s order. The posture is a rule
//! (priority 255, no flag, never asked). The sitting camel refuses to move: `is_sitting` (the occupation the posture takes)
//! stands for `refuseToMove` (the stand-up included). Camel food by tag (`#camel_food`): `cactus` is the repli (DOUTE).
use super::Def;
use crate::behaviours::{animal_panic, breed, follow_parent, float, look_at_player, posture, random_look, random_sitting, stroll, tempt, Cap, Cond, Entry};

static NOT_SITTING: Cond = Cond::Not(&Cond::Is("horizon:is_sitting"));
const FOOD: &[&str] = &["minecraft:cactus"];

pub static DEF: Def = Def {
    name: "minecraft:camel",
    goals: &[
        Entry::new(&float::MODULE, 0, &float::Params::DEFAULT),
        Entry::new(&animal_panic::MODULE, 1, &animal_panic::Params { speed: 4.0, stand_up: true, ..animal_panic::Params::DEFAULT }),
        Entry::new(&breed::MODULE, 2, &breed::Params { speed: 1.0, foods: FOOD, ..breed::Params::DEFAULT }),
        Entry::new(&tempt::MODULE, 3, &tempt::Params { speed: 2.5, items: FOOD, ..tempt::Params::DEFAULT }),
        Entry::new(&follow_parent::MODULE, 4, &follow_parent::Params { speed: 2.5 }).when(&NOT_SITTING),
        Entry::new(&stroll::MODULE, 5, &stroll::Params { speed: 2.0, ..stroll::Params::DEFAULT }).when(&NOT_SITTING),
        Entry::new(&random_sitting::MODULE, 5, &random_sitting::Params::DEFAULT),
        Entry::new(&look_at_player::MODULE, 6, &look_at_player::Params { range: 6.0, ..look_at_player::Params::DEFAULT }),
        Entry::new(&random_look::MODULE, 7, &random_look::Params::DEFAULT),
        Entry::new(&posture::MODULE, 255, &posture::Params::DEFAULT),
    ],
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Head, Cap::Locomotion, Cap::Rideable]),
    ..Def::EMPTY
};
