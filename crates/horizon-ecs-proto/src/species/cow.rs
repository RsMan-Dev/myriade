//! `Cow.registerGoals`: the base as it is.
use super::{templates::farm_animal, Def};

pub static DEF: Def = Def { name: "minecraft:cow", sounds: Some(&["minecraft:classic", "minecraft:moody"]),variant: Some(&super::variants::FARM_ANIMAL), reference: Some(&farm_animal::DEF), rules: &[crate::rules::RuleEntry::new(&crate::behaviours::held_swap::HELD_SWAP, &crate::behaviours::held_swap::SwapParams { from: "minecraft:bucket", to: "minecraft:milk_bucket", adult_only: true, takes_creature: false, sound: "minecraft:entity.cow.milk", alt: None })], ..Def::EMPTY };
