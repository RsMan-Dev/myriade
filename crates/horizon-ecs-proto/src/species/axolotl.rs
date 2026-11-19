//! `Axolotl`: its swim: amphibious navigation, `SmoothSwimmingMoveControl(85, 10, 0.1, 0.5, false)`, the push at the mob's speed, no sinking. Its goals are not configured yet (the water goals of the species are written on top of this configuration).
use super::Def;
use crate::behaviours::{Cap, Entry};
use crate::locomotion::{swim, Medium};

pub static DEF: Def = Def {
    name: "minecraft:axolotl",
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Locomotion]),
    locomotion: &[(Medium::Water, Entry::new(&swim::MODULE, 0, &swim::Params { needs_water: false, amphibious: Some(false), control: swim::Control::Smooth { max_turn_x: 85.0, max_turn_y: 10.0, in_water: 0.1, outside: 0.5, gravity: false }, push: swim::Push::Speed, sinks: false, ..swim::Params::DEFAULT }))],
    ..Def::EMPTY
};
