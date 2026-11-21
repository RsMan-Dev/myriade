//! The names a merchant's trades are looked up by: the villager type's id, and the three sets of a wandering trader.
use horizon_client_lib::mobs::VILLAGER_TYPES;

/// The three trade sets of a wandering trader.
pub const WANDERING_SETS: [&str; 3] = ["buying", "uncommon", "common"];

/// Identifiant `minecraft:` d'un type de villageois (`VILLAGER_TYPES`, par index).
pub fn variant_name(variant: u8) -> String {
    format!("minecraft:{}", VILLAGER_TYPES[usize::from(variant) % VILLAGER_TYPES.len()])
}
