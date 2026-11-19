//! `DoNothing`: runs for its duration and nothing else.
use crate::brain::*;

pub static BEHAVIOUR: Behaviour = Behaviour { name: "minecraft:do_nothing", passive: true, ..Behaviour::DEFAULT };
