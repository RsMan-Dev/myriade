//! `Frog`: its walk (and its swim, to come); its brain and the long jump are not configured yet. The long jump's flight is `Drive.discard_friction`.
use super::Def;
use crate::behaviours::{long_jump, Cap, Entry};

pub static DEF: Def = Def {
    name: "minecraft:frog",
    goals: &[Entry::new(&long_jump::MODULE, 3, &long_jump::Params::DEFAULT)],
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Head, Cap::Locomotion]),
    ..Def::EMPTY
};
