//! `Mob.aiStep`'s looting loop: a mob with `canPickUpLoot` takes what lies
//! within its box grown by `getPickupReach` (1, 0, 1) and wears or holds it
//! when it beats what it has (`equipItemIfPossible`), dropping the replaced
//! stack by its drop chance. The species whose `pickUpItem` does something
//! else keep their own loops (`pandas.rs`, `piglins.rs`, `villagers.rs`,
//! `allays.rs`, `sulfur_cubes.rs`).
use crate::{item_areas::ItemAreas, mob_level::MobLevel};
use horizon_core::{geometry::Vec3, random::RandomSource};

/// What the looting code calls on its level beyond [`MobLevel`]: the game's own
/// methods for the replace check and the attribute numbers, which mods target. Each runs
/// the function of this module of the same name.
pub trait LootingLevel: MobLevel {
}





