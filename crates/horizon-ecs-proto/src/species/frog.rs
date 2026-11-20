//! `Frog` (`horizon-client/src/mobs/frog.rs:104-140`): the goals in the `Mob`'s order. Float 0, panic (2.0) 1, breed 2 (the birth is a pregnancy:
//! `carries`), tempt with slime balls 3, shoot the tongue 2, lay spawn 2, the long jump 3, croak 4, stroll 5 (it avoids water), look 6 and 7.
//! Left out (`suivi/ecs-ecarts.md`): the poses (croak, tongue, jump) and their animations, the temptation that stands aside while pregnant, the
//! amphibious `SmoothSwimmingMoveControl`, the frog variants.
use super::Def;
use crate::behaviours::{animal_panic, breed, croak, float, lay_spawn, long_jump, look_at_player, random_look, shoot_tongue, stroll, tempt, Cap, Entry};
use crate::locomotion::{walk, Medium};

const FOOD: &[&str] = &["minecraft:slime_ball"];

pub static DEF: Def = Def {
    name: "minecraft:frog",
    goals: &[
        Entry::new(&float::MODULE, 0, &float::Params::DEFAULT),
        Entry::new(&animal_panic::MODULE, 1, &animal_panic::Params::DEFAULT),
        Entry::new(&breed::MODULE, 2, &breed::Params { foods: FOOD, carries: true, ..breed::Params::DEFAULT }),
        Entry::new(&tempt::MODULE, 3, &tempt::Params { speed: 1.25, items: FOOD, ..tempt::Params::DEFAULT }),
        Entry::new(&shoot_tongue::MODULE, 2, &shoot_tongue::Params { prey: ["minecraft:slime", "minecraft:magma_cube"], ..shoot_tongue::Params::DEFAULT }),
        Entry::new(&lay_spawn::MODULE, 2, &lay_spawn::Params::DEFAULT),
        Entry::new(&long_jump::MODULE, 3, &long_jump::Params::DEFAULT),
        Entry::new(&croak::MODULE, 4, &croak::Params::DEFAULT),
        Entry::new(&stroll::MODULE, 5, &stroll::Params { speed: 1.0, interval: Some(1), ..stroll::Params::DEFAULT }),
        Entry::new(&look_at_player::MODULE, 6, &look_at_player::Params::DEFAULT),
        Entry::new(&random_look::MODULE, 7, &random_look::Params::DEFAULT),
    ],
    // `Frog()`: `SmoothSwimmingMoveControl(85, 10, 0.02, 0.1, true)`: on land the speed attribute 1.0 is scaled by 0.1 (#2213 of the `Mob`).
    locomotion: &[(Medium::Ground, Entry::new(&walk::MODULE, 0, &walk::Params { smooth_land: Some((10.0, 0.1)), ..walk::Params::DEFAULT }))],
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Head, Cap::Locomotion]),
    ..Def::EMPTY
};
