//! `GateAll` (`Gate { shuffle: false, try_all: true }`, the tadpole's idle gate): its kids are tried in order, and every one that
//! can start does; the gate runs while a kid runs.
use crate::brain::*;

pub static BEHAVIOUR: Behaviour = Behaviour { name: "minecraft:gate_all", gate: Some(Gate { shuffle: false, try_all: true }), ..Behaviour::DEFAULT };
