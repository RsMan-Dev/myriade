use horizon_core::random::RandomSource;


/// `InfestedBlock.isCompatibleHostBlock`: the blocks an infested block can
/// be hidden in, i.e. the host side of vanilla's host-to-infested map. A
/// silverfish prefers these as a walking target.
pub fn is_infested_host(name: &str) -> bool {
    matches!(
        name,
        "minecraft:stone"
            | "minecraft:cobblestone"
            | "minecraft:stone_bricks"
            | "minecraft:mossy_stone_bricks"
            | "minecraft:cracked_stone_bricks"
            | "minecraft:chiseled_stone_bricks"
            | "minecraft:deepslate"
    )
}
/// `Silverfish.SilverfishWakeUpFriendsGoal`: 20 ticks after being hurt, the
/// infested blocks within 10 blocks sideways and 5 up or down break (or turn
/// back into stone with mob griefing off), until a coin flip stops it.
#[derive(Clone, Debug)]
struct SilverfishWakeUpFriends;


#[cfg(test)]
mod spider_vehicle_tests {
    use super::*;
    use crate::mobs::world::{test_player, with_test_world};
    use horizon_core::geometry::Vec3;
    use std::sync::Arc;

}
