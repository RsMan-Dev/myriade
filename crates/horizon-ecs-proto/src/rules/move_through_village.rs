//! Calculations of the village walk (`MoveThroughVillageGoal`): the ask grid, the section of a block, the range of the village,
//! the list of the last visited points, and the radius that ends the walk (`mobs/zombie.rs`, the village walk of the `Mob`).

/// Ticks between two asks of the goal (`canUse` every 60 ticks, its own deadline).
pub const PERIOD: i32 = 60;
/// How many visited village points are remembered.
pub const VISITED_KEEP: usize = 16;
/// The village is near when the section distance to it is at most this many sections (`ServerLevel.isCloseToVillage(pos, 6)`).
pub const CLOSE_SECTIONS: i32 = 6;

/// The first ask after an arming: one tick plus a draw below `PERIOD` (`draw(bound)` is the entity's random stream).
pub fn arm_wait(draw: impl FnOnce(i32) -> i32) -> i32 {
    1 + draw(PERIOD)
}
/// The section (16 blocks on each axis) of a block position.
pub fn section_of(block: [i32; 3]) -> [i32; 3] {
    [block[0] >> 4, block[1] >> 4, block[2] >> 4]
}
/// True when the sections to the village are within `CLOSE_SECTIONS`.
pub fn is_close_to_village(sections_to_village: i32) -> bool {
    sections_to_village <= CLOSE_SECTIONS
}
/// Records a visited point; the list keeps only its last `VISITED_KEEP` points.
pub fn remember_visited(visited: &mut Vec<[i32; 3]>, poi: [i32; 3]) {
    visited.push(poi);
    if visited.len() > VISITED_KEEP {
        visited.remove(0);
    }
}
/// The radius the walk ends at: the width of the entity plus the distance of the village (`canContinue`).
pub fn crossing_end_radius(width: f64, distance: i32) -> f64 {
    width + f64::from(distance)
}
