//! `Ghast`: a free flier (`GhastMoveControl`, no path): it floats toward spots it picks around itself (`RandomFloatAround`) and shoots a large
//! fireball at its target (`charged_shot`, priority 7). Left out: the look goal (`ghast_look`, priority 7): it requires `Cap::Head`, which this
//! species does not have (the sheet does not say to add it): DOUTE, to decide by the integrator; the target (`NearestTarget` of a player within
//! 4 blocks in height: no module for that predicate yet); the explosion that the fireball asks (`Events.exploded`) has no capability yet.
use super::Def;
use crate::behaviours::{charged_shot, float_around, Cap, Entry};
use crate::locomotion::{fly, Medium};

pub static DEF: Def = Def {
    name: "minecraft:ghast",
    goals: &[Entry::new(&float_around::MODULE, 5, &float_around::Params::DEFAULT), Entry::new(&charged_shot::MODULE, 7, &charged_shot::Params::GHAST)],
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Targets, Cap::Locomotion]),
    locomotion: &[(Medium::Air, Entry::new(&fly::MODULE, 0, &fly::Params { control: fly::Control::Float, free: true, ..fly::Params::DEFAULT }))],
    ..Def::EMPTY
};
