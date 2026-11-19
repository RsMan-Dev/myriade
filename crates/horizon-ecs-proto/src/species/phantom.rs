//! `Phantom`: a free flier (`travelFlying`) banking toward a wanted point (`PhantomMoveControl`), no path. Its goals (circle around the anchor,
//! swoop, target the player) are `behaviours/phantom_*`.
use super::Def;
use crate::behaviours::{Cap, Entry};
use crate::locomotion::{fly, Medium};

pub static DEF: Def = Def {
    name: "minecraft:phantom",
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Locomotion]),
    locomotion: &[(Medium::Air, Entry::new(&fly::MODULE, 0, &fly::Params { control: fly::Control::Bank, free: true, ..fly::Params::DEFAULT }))],
    ..Def::EMPTY
};
