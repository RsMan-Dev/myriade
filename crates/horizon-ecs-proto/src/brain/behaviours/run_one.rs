//! `RunOne` (`GateBehavior`, shuffled, one kid runs): the kids are tried in a weighted shuffle, the first that starts wins.
use crate::brain::*;

pub static BEHAVIOUR: Behaviour = Behaviour { name: "minecraft:run_one", gate: Some(Gate { shuffle: true, try_all: false, quiet: false }), ..Behaviour::DEFAULT };
/// The same gate, quiet when no kid starts (see `Gate::quiet`).
pub static QUIET: Behaviour = Behaviour { name: "minecraft:run_one", gate: Some(Gate { shuffle: true, try_all: false, quiet: true }), ..Behaviour::DEFAULT };
