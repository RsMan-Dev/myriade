//! `Armadillo` goals (`mobs/armadillo.rs:56-78`): the panic of the environment only (`ArmadilloPanic`, 1), the rolling (2), and the
//! breed (3), the tempt (4, spider eyes) and the baby following an adult (4) WHEN IDLE (`armadillo:idle`: the `Idle` wrapper).
use crate::behaviours::{baby_follow_adult, breed, float, look_at_player, panic, random_look, rolled, stroll, tempt, Cond, Entry};
use crate::species::Def;

static IDLE: Cond = Cond::Is("armadillo:idle");
const FOOD: &[&str] = &["minecraft:spider_eye"];

pub static DEF: Def = Def {
    name: "minecraft:armadillo",
    goals: &[
        Entry::new(&float::MODULE, 0, &float::Params::DEFAULT),
        Entry::new(&panic::MODULE, 1, &panic::Params { environmental: true, ..panic::Params::DEFAULT }),
        Entry::new(&rolled::MODULE, 2, &rolled::Params::DEFAULT),
        Entry::new(&breed::MODULE, 3, &breed::Params { speed: 1.0, foods: FOOD, ..breed::Params::DEFAULT }).when(&IDLE),
        Entry::new(&tempt::MODULE, 4, &tempt::Params { speed: 1.25, items: FOOD, ..tempt::Params::DEFAULT }).when(&IDLE),
        Entry::new(&baby_follow_adult::MODULE, 4, &baby_follow_adult::Params::DEFAULT).when(&IDLE),
        Entry::new(&stroll::MODULE, 5, &stroll::Params::DEFAULT),
        Entry::new(&look_at_player::MODULE, 6, &look_at_player::Params { range: 6.0, ..look_at_player::Params::DEFAULT }),
        Entry::new(&random_look::MODULE, 7, &random_look::Params::DEFAULT),
    ],
    ..Def::EMPTY
};
