//! `Strider`: walks on the ground and ON lava (the lava mode: its path map has lava as ground, the lava holds it up, it floats). The goals of
//! the species (`StriderGoToLava`...) are not declared here: `go_to` moves it.
use super::Def;
use crate::behaviours::{Cap, Entry};
use crate::locomotion::{lava, walk, Medium};

pub static DEF: Def = Def {
    name: "minecraft:strider",
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Locomotion]),
    locomotion: &[(Medium::Ground, Entry::new(&walk::MODULE, 0, &walk::Params::DEFAULT)), (Medium::Lava, Entry::new(&lava::MODULE, 0, &lava::Params::DEFAULT))],
    ..Def::EMPTY
};
