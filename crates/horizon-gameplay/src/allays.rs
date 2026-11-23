//! Allays on the server side: spotting and picking up matching drops, and
//! throwing their load to the liked player or note block (`AllayAi`).
use crate::mob_level::MobLevel;
use myriade_engine::{
    items::ItemStack,
    mobs::{Species, allay},
};
use horizon_core::{geometry::Vec3, random::RandomSource};

/// What allay code calls on its level beyond [`MobLevel`]: the game's own allay
/// methods, which mods target. Each runs the function of this module of the same name.
pub trait AllayLevel: MobLevel {
    /// `GoAndGiveItemsToTarget` / `AllayAi.throwItem` ([`deliver`]).
    fn allay_deliver(&mut self, i: usize);
    /// `AllayAi.getLikedPlayer` ([`liked_player`]).
    fn allay_liked_player(&self, i: usize) -> Option<Vec3>;
    /// The UUID of the player an allay likes ([`liked_uuid`]).
    fn allay_liked_uuid(&self, i: usize) -> [i32; 4];
}




