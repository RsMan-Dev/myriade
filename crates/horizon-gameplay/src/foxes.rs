//! Foxes and dolphins on the game side: what they take from the ground (`Fox.pickUpItem`,
//! `Fox.canHoldItem`, `Dolphin.pickUpItem`) and where `FoxSearchForItemsGoal` and
//! `Dolphin.MoveToItemGoal` walk to.
use crate::{item_areas::ItemAreas, mob_level::MobLevel};
use horizon_core::geometry::Vec3;

/// What fox code calls on its level beyond [`MobLevel`]: the game's own fox
/// methods, which mods target. Each runs the function of this module of the same name.
pub trait FoxLevel: MobLevel {
    /// `Fox.isConsumableFood` ([`fox_food`]).
    fn fox_food(&self, item: &str) -> bool;
    /// `Fox.pickUpItem` ([`fox_pick_up`]).
    fn fox_pick_up(&mut self, fox_index: usize, item_index: usize);
}




