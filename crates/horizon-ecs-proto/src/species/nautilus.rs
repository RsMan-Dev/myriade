//! `Nautilus` (`nautilus.rs:40-71`), the swimmer of the charge: `SmoothSwimming(85, 10, 0.011, 0, true)` (`nautilus.rs:40-47`),
//! the charge of priority 3 at speed 0,6 (`nautilus.rs:68`). Left out, waiting for their module: the animal panic (priority 0),
//! the breed (1), the tempt (2), the stroll (4), and the target rule (`find_target`, the anger, the pufferfish) that the test sets.
use super::Def;
use crate::behaviours::{charge_attack, Cap, Entry};
use crate::locomotion::{swim, Medium};

pub static DEF: Def = Def {
    name: "minecraft:nautilus",
    goals: &[Entry::new(&charge_attack::MODULE, 3, &charge_attack::Params { speed: 0.6, ..charge_attack::Params::DEFAULT })],
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Head, Cap::Targets, Cap::Locomotion]),
    locomotion: &[(Medium::Water, Entry::new(&swim::MODULE, 0, &swim::Params { needs_water: false, breach: false, control: swim::Control::Smooth { max_turn_x: 85.0, max_turn_y: 10.0, in_water: 0.011, outside: 0.0, gravity: true }, push: swim::Push::Speed, sinks: true, ..swim::Params::DEFAULT }))],
    ..Def::EMPTY
};
