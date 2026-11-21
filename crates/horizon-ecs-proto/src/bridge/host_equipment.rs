//! The equipment stacks through the bridge (`equip_stack.rs`): the stack of a slot written by entity (the read is `equipment_stack`, in
//! `host_read_living.rs`), the wear, the changes and the breaks the host reads once per tick, and the equip sound.
use crate::bridge::host::EcsWorld;
use crate::equip_stack::{self, EQUIP_GENERIC, SLOTS};
use bevy_ecs::entity::Entity;
use horizon_client_lib::items::ItemStack;

impl EcsWorld {
    /// Puts `stack` in `slot` of `e`; false when `e` is gone or `slot` is out of range.
    pub fn set_equipment_stack(&mut self, e: Entity, slot: usize, stack: ItemStack) -> bool {
        equip_stack::set(self.world_mut(), e, slot, Some(stack))
    }
    /// Wear of `amount` points on `slot` of `e` (`equip_stack::damage`): the piece breaks at its durability. False when nothing wears there.
    pub fn damage_equipment(&mut self, e: Entity, slot: usize, amount: i32) -> bool {
        equip_stack::damage(self.world_mut(), e, slot, amount)
    }
    /// The sound of an equip of `slot` on `e`: the item's own (`over`, from its equippable component), else the generic one. None when `e`
    /// is gone or `slot` is out of range.
    pub fn equip_sound(&self, e: Entity, slot: usize, over: Option<String>) -> Option<String> {
        if slot >= SLOTS || self.world().get_entity(e).is_err() {
            return None;
        }
        Some(over.unwrap_or_else(|| EQUIP_GENERIC.to_string()))
    }
}
