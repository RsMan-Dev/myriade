//! `minecraft:arrow` and `minecraft:spectral_arrow`: the shot arrows, saved with their world (`EntityType` without `noSave`). Their flight
//! and hit are in `projectile.rs` and `arrow_more.rs`; the species only names them, so a saved arrow is restored by its id.
use super::Def;

pub static ARROW: Def = Def { name: "minecraft:arrow", ..Def::EMPTY };
pub static SPECTRAL_ARROW: Def = Def { name: "minecraft:spectral_arrow", ..Def::EMPTY };
