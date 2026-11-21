//! The game side of the looting pickup: the trigger and the equipment numbers piglins share.
use super::Game;
use horizon_client::{
    entities::ItemEntity,
    items::ItemStack,
    mobs::{
        Mob,
        equip::Slot,
    },
};

impl Game<'_> {
    /// `Mob.getApproximateAttributeWith` for `stack` in `slot`: each
    /// attribute starts from the mob's own value. (armour, toughness, damage)
    fn attributes(&self, m: &Mob, stack: &ItemStack, slot: Slot) -> (f64, f64, f64) {
        horizon_gameplay::mob_looting::attributes(self.items(), m, stack, slot)
    }

    /// `LivingEntity.onItemPickup`: the advancement of whoever threw the item,
    /// whichever mob took it.
    pub(super) fn mob_item_pickup_trigger(&mut self, index: usize, e: &ItemEntity) {
        let Some(owner) = e.thrower else {
            return;
        };
        let stack = e.stack.clone();
        let target = self.mob_entity(&self.remote_entities[index]);
        self.for_each_player(|g| {
            if g.acting_uuid() == owner {
                g.trigger_item_entity("minecraft:thrown_item_picked_up_by_entity", &stack, &target);
            }
        });
    }
}
