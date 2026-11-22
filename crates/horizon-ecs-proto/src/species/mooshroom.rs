//! `Mooshroom` (`farm.rs:35`): the cow's goals. Left out: the walk value of the mycelium (`ai.rs:3543`: `WalkValue` has no such
//! variant) and the interactions (bowl, shears: writes the player's equipment, a framework service not ready).
use super::{cow, Def};
use crate::behaviours::shear::{adult, ShearDef};
use crate::behaviours::variant::Variant;
use crate::conversion::{convert_or_ask, ConvertKeep};

/// `MushroomCow.shear`: the cow it becomes (nothing kept) and 5 mushrooms of its variant (`Variant` 1: brown) fall from its height.
pub static SHEARING: ShearDef = ShearDef {
    equip: |_, _| {},
    ready: |w, e| adult(w, e),
    apply: |w, e| convert_or_ask(w, e, "minecraft:cow", ConvertKeep::NONE),
    table: |w, e| Some(format!("minecraft:shearing/mooshroom/{}", if w.get::<Variant>(e).is_some_and(|v| v.0 == 1) { "brown" } else { "red" })),
    sound: "minecraft:entity.mooshroom.shear",
    height: None,
    puff: Some("minecraft:explosion"),
};

pub static DEF: Def = Def { name: "minecraft:mooshroom", reference: Some(&cow::DEF), rules: &[crate::rules::RuleEntry::new(&crate::behaviours::shear::SHEAR, &SHEARING), crate::rules::RuleEntry::new(&crate::behaviours::stew_flower::STEW_FLOWER, &()), crate::rules::RuleEntry::new(&crate::behaviours::stew_flower::STEW_BOWL, &()), crate::rules::RuleEntry::new(&crate::behaviours::held_swap::HELD_SWAP, &crate::behaviours::held_swap::SwapParams { from: "minecraft:bowl", to: "minecraft:mushroom_stew", adult_only: true, takes_creature: false, sound: "minecraft:entity.mooshroom.milk", alt: None })], ..Def::EMPTY };
