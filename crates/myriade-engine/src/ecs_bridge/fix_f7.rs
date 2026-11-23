//! Lot F7: the jump charge of a mob mount, cleared when a rider mounts it (`Entity.startRiding` resets the bar).
use super::level::EcsLevel;
use myriade_engine::items::ItemStack;

impl EcsLevel {
    /// Ends the jump charge of the mount `id`, if it has one: `charge_jump` released with no key held.
    pub fn clear_jump_charge(&mut self, id: u32) {
        let Some(ecs) = self.ecs_mut() else {
            return;
        };
        if let Some(e) = ecs.entity_of(id) {
            ecs.charge_jump(e, false);
        }
    }
    /// `LivingEntity.setItemSlot(MAINHAND)` on the mob `id`: the hand stack the item use reads, beside its `Equipment` slot.
    pub fn set_main_hand_of(&mut self, id: u32, stack: Option<ItemStack>) {
        if let Some(e) = self.entity_of(id) {
            self.set_main_hand(e, stack);
        }
    }
}
