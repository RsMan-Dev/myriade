//! The base of the fish (`AbstractFish.registerGoals`, the part that is ported): random swimming at priority 4 (the panic at 0, the
//! avoidance of the player at 2 and the school are not ported yet). A fish has no targets: no `Targets` capability. It swims in the
//! water and walks (falls, flops) out of it: both modes, one active.
use crate::behaviours::{fish_swim, Cap, Entry};
use crate::locomotion::{swim, walk, Medium};
use crate::species::Def;

pub static DEF: Def = Def {
    name: "template:fish",
    reference: None,
    targets: &[],
    goals: &[Entry::new(&fish_swim::MODULE, 4, &fish_swim::Params::DEFAULT)],
    changes: &[],
    rules: &[],
    brain: crate::brain::BrainDef::EMPTY,
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Locomotion]),
    locomotion: &[
        (Medium::Ground, Entry::new(&walk::MODULE, 0, &walk::Params::DEFAULT)),
        (Medium::Water, Entry::new(&swim::MODULE, 0, &swim::Params::DEFAULT)),
    ],
};
