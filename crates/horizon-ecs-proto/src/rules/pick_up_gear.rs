//! Calculations of the gear pickup (`PickUpGear`): the chance to drop the replaced item, which is doubled for a slot already
//! looted, and whether a draw drops it.

/// The chance that the replaced item drops: twice the base for a slot already looted, else the base.
pub fn drop_chance(looted_here: bool, base: f32) -> f32 {
    if looted_here { 2.0 } else { base }
}
/// The replaced item drops when the draw, less 0.1 and not under zero, is under the chance.
pub fn drops_old(draw: f32, chance: f32) -> bool {
    (draw - 0.1).max(0.0) < chance
}
