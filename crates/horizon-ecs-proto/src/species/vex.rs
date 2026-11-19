//! `Vex`: flies through the blocks (`noPhysics`) straight at a wanted point (`VexMoveControl`), no path. Its goals: charge, random move, copy
//! the owner's target (`behaviours/vex_*`).
use super::Def;
use crate::behaviours::{Cap, Entry};
use crate::locomotion::{fly, Medium};

pub static DEF: Def = Def {
    name: "minecraft:vex",
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Locomotion]),
    locomotion: &[(Medium::Air, Entry::new(&fly::MODULE, 0, &fly::Params { control: fly::Control::Direct, noclip: true, ..fly::Params::DEFAULT }))],
    ..Def::EMPTY
};
