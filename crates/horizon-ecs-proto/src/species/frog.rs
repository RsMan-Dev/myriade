//! `Frog` (`horizon-client/src/mobs/frog.rs:104-140`): the goals in the `Mob`'s order. Float 0, panic (2.0) 1, breed 2 (the birth is a pregnancy:
//! `carries`), tempt with slime balls 3, shoot the tongue 2, lay spawn 2, the long jump 3, croak 4, stroll 5 (it avoids water), look 6 and 7.
//! Water: the amphibious smooth swimming control (`reset_pitch`: its look control resets xRot each tick). The temptation and the long jump stand aside while pregnant.
//! Left out (`suivi/ecs-ecarts.md`): the poses (croak, tongue, jump) and their animations, the frog variants; no swim activity (the `Mob` has none).
use super::Def;
use crate::behaviours::{animal_panic, breed, croak, float, lay_spawn, long_jump, look_at_player, random_look, shoot_tongue, stroll, tempt, try_find_land, Cap, Entry};
use crate::locomotion::{swim, walk, Medium};
use crate::behaviours::Cond;

static NOT_PREGNANT: Cond = Cond::Not(&Cond::Is("frog:pregnant"));
static IN_WATER: Cond = Cond::Is("horizon:in_water");
static OUT_OF_WATER: Cond = Cond::Not(&IN_WATER);

const FOOD: &[&str] = &["minecraft:slime_ball"];

pub static DEF: Def = Def {
    variant: Some(&super::variants::FROG),
    name: "minecraft:frog",
    goals: &[
        Entry::new(&float::MODULE, 0, &float::Params::DEFAULT),
        Entry::new(&animal_panic::MODULE, 1, &animal_panic::Params::DEFAULT),
        Entry::new(&breed::MODULE, 2, &breed::Params { foods: FOOD, carries: true, ..breed::Params::DEFAULT }),
        Entry::new(&tempt::MODULE, 3, &tempt::Params { speed: 1.25, items: FOOD, ..tempt::Params::DEFAULT }).when(&NOT_PREGNANT),
        Entry::new(&shoot_tongue::MODULE, 2, &shoot_tongue::Params { prey: ["minecraft:slime", "minecraft:magma_cube"], ..shoot_tongue::Params::DEFAULT }),
        Entry::new(&lay_spawn::MODULE, 2, &lay_spawn::Params::DEFAULT),
        // `FrogAi.initJumpActivity` comes after the lay spawn activity (IS_PREGNANT): a pregnant frog does not leap.
        Entry::new(&long_jump::MODULE, 3, &long_jump::Params::DEFAULT).when(&NOT_PREGNANT),
        // The SWIM activity (`IS_IN_WATER`): `TryFindLand(8, 1.5)` finds the bank. The IDLE `TryFindLand(6, 1.0)` is not written:
        // the module holds one state per animal, and the swim entry takes it.
        Entry::new(&try_find_land::MODULE, 3, &try_find_land::Params { range: 8, speed: 1.5 }).when(&IN_WATER),
        Entry::new(&croak::MODULE, 4, &croak::Params::DEFAULT).when(&OUT_OF_WATER),
        Entry::new(&stroll::MODULE, 5, &stroll::Params { speed: 1.0, interval: Some(1), ..stroll::Params::DEFAULT }).when(&OUT_OF_WATER),
        Entry::new(&look_at_player::MODULE, 6, &look_at_player::Params::DEFAULT),
        Entry::new(&random_look::MODULE, 7, &random_look::Params::DEFAULT),
    ],
    // `Frog()`: `SmoothSwimmingMoveControl(85, 10, 0.02, 0.1, true)`: on land the speed attribute 1.0 is scaled by 0.1 (#2213 of the `Mob`).
    // In the water the same control (`SmoothSwimmingMoveControl`, 0.02 in water) on the amphibious navigation that prefers the shallows.
    locomotion: &[
        (Medium::Water, Entry::new(&swim::MODULE, 0, &swim::Params { needs_water: false, amphibious: Some(true), control: swim::Control::Smooth { max_turn_x: 85.0, max_turn_y: 10.0, in_water: 0.02, outside: 0.1, gravity: true }, push: swim::Push::Speed, sinks: false, reset_pitch: true, ..swim::Params::DEFAULT })),
        (Medium::Ground, Entry::new(&walk::MODULE, 0, &walk::Params { smooth_land: Some((10.0, 0.1)), ..walk::Params::DEFAULT })),
    ],
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Head, Cap::Locomotion]),
    ..Def::EMPTY
};
