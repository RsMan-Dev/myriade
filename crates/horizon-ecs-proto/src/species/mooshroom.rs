//! `Mooshroom` (`farm.rs:35`): the cow's goals. Left out: the walk value of the mycelium (`ai.rs:3543`: `WalkValue` has no such
//! variant) and the interactions (bowl, shears: writes the player's equipment, a framework service not ready).
use super::{cow, Def};

pub static DEF: Def = Def { name: "minecraft:mooshroom", reference: Some(&cow::DEF), ..Def::EMPTY };
