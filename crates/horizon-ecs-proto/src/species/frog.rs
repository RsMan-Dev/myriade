//! `Frog`: its walk (and its swim, to come); its brain and the long jump are not configured yet. The long jump's flight is `Drive.discard_friction`.
use super::Def;
use crate::behaviours::Cap;

pub static DEF: Def = Def {
    name: "minecraft:frog",
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Locomotion]),
    ..Def::EMPTY
};
