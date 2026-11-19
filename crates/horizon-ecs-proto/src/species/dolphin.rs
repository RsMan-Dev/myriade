//! `Dolphin`: its swim: water-bound navigation that may breach, `SmoothSwimmingMoveControl(85, 10, 0.02, 0.1, true)`, the push at the mob's speed. Its goals are not configured yet (the water goals of the species are written on top of this configuration).
use super::Def;
use crate::behaviours::{Cap, Entry};
use crate::locomotion::{swim, Medium};

pub static DEF: Def = Def {
    name: "minecraft:dolphin",
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Locomotion]),
    locomotion: &[(Medium::Water, Entry::new(&swim::MODULE, 0, &swim::Params { needs_water: false, breach: true, control: swim::Control::Smooth { max_turn_x: 85.0, max_turn_y: 10.0, in_water: 0.02, outside: 0.1, gravity: true }, push: swim::Push::Speed, sinks: true, ..swim::Params::DEFAULT }))],
    ..Def::EMPTY
};
