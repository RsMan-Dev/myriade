//! `Frog`: its walk (and its swim, to come); its brain and the long jump are not configured yet. The long jump's flight is `Drive.discard_friction`.
use super::Def;
use crate::behaviours::{long_jump, look_at_player, random_look, Cap, Entry};

pub static DEF: Def = Def {
    name: "minecraft:frog",
    // The look goals of `frog.rs:139-140` (the other goals of the Mob's list are not configured yet).
    goals: &[
        Entry::new(&long_jump::MODULE, 3, &long_jump::Params::DEFAULT),
        Entry::new(&look_at_player::MODULE, 6, &look_at_player::Params::DEFAULT),
        Entry::new(&random_look::MODULE, 7, &random_look::Params::DEFAULT),
    ],
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Head, Cap::Locomotion]),
    ..Def::EMPTY
};
