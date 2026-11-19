//! `Ghast` (`mobs/nether.rs:201-216`): a free flier (`GhastMoveControl`, no path): it floats toward spots it picks around itself
//! (`RandomFloatAround`), looks at its target (`GhastLook`, priority 7) and shoots a large fireball at it (`charged_shot`, priority 7). Left out:
//! the player target (priority 1, only when the player is within 4 blocks in height: `nearest_target` has no height filter), the explosion
//! of the fireball (`Events.exploded` has no capability yet).
use super::Def;
use crate::behaviours::{charged_shot, float_around, ghast_look, Cap, Entry};
use crate::locomotion::{fly, Medium};

pub static DEF: Def = Def {
    name: "minecraft:ghast",
    goals: &[
        Entry::new(&float_around::MODULE, 5, &float_around::Params::DEFAULT),
        Entry::new(&ghast_look::MODULE, 7, &ghast_look::Params::DEFAULT),
        Entry::new(&charged_shot::MODULE, 7, &charged_shot::Params::GHAST),
    ],
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Head, Cap::Targets, Cap::Locomotion]),
    locomotion: &[(Medium::Air, Entry::new(&fly::MODULE, 0, &fly::Params { control: fly::Control::Float, free: true, ..fly::Params::DEFAULT }))],
    ..Def::EMPTY
};
