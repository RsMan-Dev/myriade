//! Calculations of the climb on powder snow: the block of the feet, and the test of the block above them.
use horizon_client_lib::mobs::MobWorld;
use horizon_core::geometry::Vec3;

/// The block the animal stands in: the floor of its position.
pub fn feet_block(p: Vec3) -> [i32; 3] {
    [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32]
}

/// `Mob.blockPosition().above()` is powder snow, or has no collision.
pub fn above_passable(mw: &MobWorld<'_>, feet: [i32; 3]) -> bool {
    let above = [feet[0], feet[1] + 1, feet[2]];
    mw.block_name(above[0], above[1], above[2]) == "minecraft:powder_snow" || mw.collision_bounds(above).is_none()
}
