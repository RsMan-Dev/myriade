//! `BigDripleafBlock.entityInside` fires for every entity whose box is inside
//! the leaf, and `BlockBehaviour.getEntityInsideCollisionShape` is the whole
//! block, so a motionless mob, a dropped item, a boat, a minecart and a trident
//! standing on one tilt it exactly as a player does: UNSTABLE, PARTIAL ten
//! ticks later, FULL ten ticks after that, and back to NONE 100 ticks after
//! FULL.

use super::Game;
use super::block_ticks::standing_feet;
use super::boats::Boat;
use super::game_tests::tests::server_game;
use super::minecarts::{CartKind, Minecart};
use super::trident::ThrownTrident;
use horizon_client::chunk::Chunk;
use horizon_client::entities::ItemEntity;
use horizon_client::items::ItemStack;
use horizon_client::mobs::Mob;
use horizon_core::geometry::Vec3;
use std::sync::Arc;

#[test]
fn feet_exclude_a_leaf_only_touched_by_the_undeflated_box() {
    let mut standing = Vec::new();
    standing_feet(&mut standing, [1.3 - 0.000005, 64.9, 0.5], true, 0.3);
    assert_eq!(standing.len(), 1);
    assert_eq!(standing[0].0, horizon_core::position::BlockPos::new(1, 64, 0));
}
