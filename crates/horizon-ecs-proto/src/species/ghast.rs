//! `Ghast`: a free flier (`GhastMoveControl`, no path): it floats toward spots it picks around itself (`RandomFloatAround`). The look
//! goal, the target and the fireball are not ported yet.
use super::Def;
use crate::behaviours::{float_around, Cap, Entry};
use crate::locomotion::{fly, Medium};

pub static DEF: Def = Def {
    name: "minecraft:ghast",
    goals: &[Entry::new(&float_around::MODULE, 5, &float_around::Params::DEFAULT)],
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Locomotion]),
    locomotion: &[(Medium::Air, Entry::new(&fly::MODULE, 0, &fly::Params { control: fly::Control::Float, free: true, ..fly::Params::DEFAULT }))],
    ..Def::EMPTY
};
