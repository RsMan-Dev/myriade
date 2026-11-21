//! `GateAll` (`Gate { shuffle: false, try_all: true, quiet: false }`, the tadpole's idle gate): its kids are tried in order, and every one that
//! can start does; the gate runs while a kid runs.
use crate::brain::*;

pub static BEHAVIOUR: Behaviour = Behaviour { name: "minecraft:gate_all", gate: Some(Gate { shuffle: false, try_all: true, quiet: false }), ..Behaviour::DEFAULT };
/// The same gate, quiet when no kid starts (see `Gate::quiet`).
pub static QUIET: Behaviour = Behaviour { name: "minecraft:gate_all", gate: Some(Gate { shuffle: false, try_all: true, quiet: true }), ..Behaviour::DEFAULT };
