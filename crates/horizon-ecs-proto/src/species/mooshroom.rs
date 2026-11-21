//! `Mooshroom` (`farm.rs:35`): the cow's goals. Left out: the walk value of the mycelium (`ai.rs:3543`: `WalkValue` has no such
//! variant) and the interactions (bowl, shears: writes the player's equipment, a framework service not ready).
use super::{cow, Def};

pub static DEF: Def = Def { name: "minecraft:mooshroom", reference: Some(&cow::DEF), rules: &[crate::rules::RuleEntry::new(&crate::behaviours::held_swap::HELD_SWAP, &crate::behaviours::held_swap::SwapParams { from: "minecraft:bowl", to: "minecraft:mushroom_stew", adult_only: true, takes_creature: false })], ..Def::EMPTY };
