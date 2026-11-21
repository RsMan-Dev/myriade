//! `SnowGolem` (`golem.rs:548-565`): a ranged attack with snowballs, looking at players and random looks. Left out, waiting for their
//! predicate, parameter or module: the hostile target (`Prey::Filter(hostile)`), the stroll with its water probability (`stroll` has no
//! such parameter), (the snow trail is in, at 255: no flag).
use super::Def;
use crate::behaviours::{look_at_player, nearest_target, random_look, ranged_attack, snow_trail, stroll, Entry};

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
    ..Def::EMPTY
};
