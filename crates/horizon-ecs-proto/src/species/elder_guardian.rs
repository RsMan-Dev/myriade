//! `Elder Guardian`: the guardian with a 60-tick beam that goes on at any distance, +2 magic damage, a stroll every 400 ticks.
use super::Def;
use crate::behaviours::{guardian_beam, look_at_player, move_towards_restriction, random_look, stroll, Cap, Entry};
use crate::locomotion::{swim, Medium};

pub static DEF: Def = Def {
    name: "minecraft:elder_guardian",
    targets: super::guardian::TARGETS,
    goals: &[
        Entry::new(&guardian_beam::MODULE, 4, &guardian_beam::Params::ELDER),
        Entry::new(&move_towards_restriction::MODULE, 5, &move_towards_restriction::Params::DEFAULT),
        Entry::new(&stroll::MODULE, 7, &stroll::Params { interval: Some(400), ..stroll::Params::DEFAULT }),
        Entry::new(&look_at_player::MODULE, 8, &look_at_player::Params { range: 8.0, ..look_at_player::Params::DEFAULT }),
        Entry::new(&random_look::MODULE, 9, &random_look::Params::DEFAULT),
    ],
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Head, Cap::Targets, Cap::Locomotion]),
    locomotion: &[(Medium::Water, Entry::new(&swim::MODULE, 0, &swim::Params { needs_water: true, ..swim::Params::DEFAULT }))],
    ..Def::EMPTY
};
