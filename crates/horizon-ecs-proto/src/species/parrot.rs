//! `Parrot`: a flier (`FlyingMoveControl(10, false)`, `FlyingPathNavigation`) that wanders in the air; its other goals (the owner, the
//! shoulder, the mimic, the panic) are not ported yet.
use super::Def;
use crate::behaviours::{fly_stroll, Cap, Entry};
use crate::locomotion::{fly, Medium};

pub static DEF: Def = Def {
    name: "minecraft:parrot",
    reference: None,
    targets: &[],
    goals: &[Entry::new(&fly_stroll::MODULE, 2, &fly_stroll::Params::DEFAULT)],
    changes: &[],
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Locomotion]),
    locomotion: &[(Medium::Air, Entry::new(&fly::MODULE, 0, &fly::Params { max_turn: 10.0, hovers: false, glide: 0.6 }))],
};
