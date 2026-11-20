//! `Guardian` movement (`mobs/guardian.rs:39-60`, `GuardianMoveControl`, `Guardian.travelInWater`): `swim::Control::Guardian` with the push 0.1 and
//! drag 0.9; goals: `MoveTowardsRestriction` (5, with LOOK), the plain stroll (no water avoiding: `water_avoiding: 1.0`) every 80 ticks (7), the look at players (8) and the random look (9).
//! Pending, named (the ray attack's porter): `GuardianAttack` (4), the target selector (players, squid and axolotls beyond 3 blocks) and the
//! look at other guardians (range 12, chance 0.01: `look_at_player` has no species filter yet).
use super::Def;
use crate::behaviours::{look_at_player, move_towards_restriction, random_look, stroll, Cap, Entry};
use crate::locomotion::{swim, Medium};

pub static DEF: Def = Def {
    name: "minecraft:guardian",
    goals: &[
        Entry::new(&move_towards_restriction::MODULE_LOOK, 5, &move_towards_restriction::Params { speed: 1.0 }),
        Entry::new(&stroll::MODULE, 7, &stroll::Params { interval: Some(80), water_avoiding: 1.0, ..stroll::Params::DEFAULT }),
        Entry::new(&look_at_player::MODULE, 8, &look_at_player::Params { range: 8.0, ..look_at_player::Params::DEFAULT }),
        Entry::new(&random_look::MODULE, 9, &random_look::Params::DEFAULT),
    ],
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Head, Cap::Locomotion]),
    locomotion: &[(Medium::Water, Entry::new(&swim::MODULE, 0, &swim::Params { needs_water: false, breach: false, control: swim::Control::Guardian, push: swim::Push::Fixed(0.1), sinks: true, ..swim::Params::DEFAULT }))],
    ..Def::EMPTY
};
