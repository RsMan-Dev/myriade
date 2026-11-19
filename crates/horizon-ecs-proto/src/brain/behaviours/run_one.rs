//! `RunOne` (`GateBehavior`, shuffled, one kid runs): the kids are tried in a weighted shuffle, the first that starts wins.
use crate::brain::*;

pub static BEHAVIOUR: Behaviour = Behaviour { name: "minecraft:run_one", gate: Some(Gate { shuffle: true, try_all: false }), ..Behaviour::DEFAULT };
