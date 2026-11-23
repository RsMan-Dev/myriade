use crate::{containers::BlockEntity, items::ItemStack};
use horizon_core::java_hash_map::{JavaHashMap, JavaKey};
use horizon_core::{
    geometry::{Aabb, Vec3},
    random::RandomSource,
};

/// `TransportItemState`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Travelling,
    Queuing,
    Interacting,
}
/// `ContainerInteractionState`.
#[allow(clippy::enum_variant_names)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Interaction {
    PickupItem,
    PickupNoItem,
    PlaceItem,
    PlaceNoItem,
}

/// `TransportItemTarget`: the chest and its block state when it was chosen.
#[derive(Clone, Debug)]
struct Target {
    pos: [i32; 3],
    name: String,
    /// The halves of the container in slot order (the right half first).
    halves: Vec<[i32; 3]>,
}

#[derive(Clone, Debug)]
pub(super) struct Transport {
    pub(super) running: bool,
    end: i64,
    target: Option<Target>,
    state: State,
    interaction: Option<Interaction>,
    ticks: i32,
}
impl Default for Transport {
    fn default() -> Self {
        Self {
            running: false,
            end: 0,
            target: None,
            state: State::Travelling,
            interaction: None,
            ticks: 0,
        }
    }
}


/// `ChestBlock.getOpenChestSound` / `getCloseChestSound` of a chest block:
/// a copper chest's comes from `CopperChestBlock.getHingeSound`, by its
/// weather state (a waxed chest's is its state's).
pub fn chest_hinge_sound(block: &str, open: bool) -> &'static str {
    let base = block
        .trim_start_matches("minecraft:")
        .trim_start_matches("waxed_");
    match (base.ends_with("copper_chest"), base.split('_').next(), open) {
        (false, _, true) => "minecraft:block.chest.open",
        (false, _, false) => "minecraft:block.chest.close",
        (true, Some("weathered"), true) => "minecraft:block.copper_chest_weathered.open",
        (true, Some("weathered"), false) => "minecraft:block.copper_chest_weathered.close",
        (true, Some("oxidized"), true) => "minecraft:block.copper_chest_oxidized.open",
        (true, Some("oxidized"), false) => "minecraft:block.copper_chest_oxidized.close",
        (true, _, true) => "minecraft:block.copper_chest.open",
        (true, _, false) => "minecraft:block.copper_chest.close",
    }
}
fn is_chest_block(name: &str) -> bool {
    matches!(name, "minecraft:chest" | "minecraft:trapped_chest") || name.ends_with("copper_chest")
}

/// `BlockPos` keys in a chunk's block entity map.
#[derive(PartialEq, Eq)]
struct PosKey([i32; 3]);
impl JavaKey for PosKey {
    fn java_hash(&self) -> i32 {
        // Vec3i.hashCode.
        (self.0[1].wrapping_add(self.0[2].wrapping_mul(31)))
            .wrapping_mul(31)
            .wrapping_add(self.0[0])
    }
    fn tie_break(&self, _: &Self) -> std::cmp::Ordering {
        std::cmp::Ordering::Less
    }
}


impl Transport {
}
