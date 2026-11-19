//! `Drowned`: its swim: amphibious navigation, `DrownedMoveControl` (the marker `WantsToSwim` makes it swim, else the ground's control), the push 0.01 only under water when it wants to. Its goals are not configured yet (the water goals of the species are written on top of this configuration).
use super::Def;
use crate::behaviours::{Cap, Entry};
use crate::locomotion::{swim, Medium};

pub static DEF: Def = Def {
    name: "minecraft:drowned",
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Locomotion]),
    locomotion: &[(Medium::Water, Entry::new(&swim::MODULE, 0, &swim::Params { needs_water: false, amphibious: Some(false), control: swim::Control::Drowned, sinks: false, ..swim::Params::DEFAULT }))],
    ..Def::EMPTY
};
