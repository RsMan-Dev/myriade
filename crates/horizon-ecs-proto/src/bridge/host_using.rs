//! The item use through the bridge (`using.rs`): the start, the ticks left, the stop and the release, the totem, the draws, the fire and the
//! roar. The events are queued by `using.rs` and drained by `using_events`.
use crate::bridge::host::EcsWorld;
use crate::equip_stack;
use crate::using::{self, OFF};
use bevy_ecs::entity::Entity;
use horizon_client_lib::items::ItemStack;

impl EcsWorld {
    /// A float drawn for `e` this tick; None when `e` is gone.
    pub fn next_float(&mut self, e: Entity) -> Option<f32> {
        using::next_float(self.world(), e)
    }
    /// An int in `0..bound` drawn for `e` this tick; None when `e` is gone or `bound` is not positive.
    pub fn next_int_bound(&mut self, e: Entity, bound: i32) -> Option<i32> {
        using::next_int_bound(self.world(), e, bound)
    }
    /// The stack of the off hand of `e` (None: empty).
    pub fn off_hand_stack(&self, e: Entity) -> Option<ItemStack> {
        equip_stack::get(self.world(), e, OFF)
    }
    pub fn is_using_item(&self, e: Entity) -> bool {
        using::state(self.world(), e).is_some()
    }
    /// Starts a use with the off hand or the main hand of `e`; false when one is running or the hand is empty.
    pub fn start_using(&mut self, e: Entity, off_hand: bool) -> bool {
        using::start(self.world_mut(), e, off_hand)
    }
    /// The use of `e` as (off hand, ticks left); None when it uses nothing.
    pub fn using(&self, e: Entity) -> Option<(bool, i32)> {
        using::state(self.world(), e)
    }
    /// Sets the ticks left of the use of `e` and arms its deadline.
    pub fn set_using(&mut self, e: Entity, counter: i32) {
        using::set_remaining(self.world_mut(), e, counter);
    }
    pub fn stop_using(&mut self, e: Entity) {
        using::stop(self.world_mut(), e);
    }
    pub fn release_using_item(&mut self, e: Entity) {
        using::release(self.world_mut(), e);
    }
    pub fn item_use_remaining(&self, e: Entity) -> Option<i32> {
        using::state(self.world(), e).map(|u| u.1)
    }
    pub fn set_item_use_remaining(&mut self, e: Entity, ticks: i32) {
        using::set_remaining(self.world_mut(), e, ticks);
    }
    pub fn using_off_hand(&self, e: Entity) -> Option<bool> {
        using::state(self.world(), e).map(|u| u.0)
    }
    pub fn ignite(&mut self, e: Entity, ticks: i32) {
        using::ignite(self.world_mut(), e, ticks);
    }
    pub fn fire_immune(&self, e: Entity) -> bool {
        using::fire_immune(self.world(), e)
    }
    pub fn is_roaring(&self, e: Entity) -> bool {
        using::is_roaring(self.world(), e)
    }
    /// The roar of `e` for `ticks`.
    pub fn roar(&mut self, e: Entity, ticks: i32) {
        using::roar(self.world_mut(), e, ticks);
    }
    /// A totem of `e` saves it from its lethal hit; false when no hand holds one.
    pub fn try_totem(&mut self, e: Entity) -> bool {
        using::try_totem(self.world_mut(), e)
    }
}
