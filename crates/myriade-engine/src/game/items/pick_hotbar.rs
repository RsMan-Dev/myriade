//! Pick Block into the acting player's inventory, asking
//! `ItemPart::is_not_replaceable_by_pick_action` in `Inventory.getSuitableHotbarSlot`
//! (`myriade_engine::pick_item`).
use super::*;

impl Game<'_> {
    /// `ServerGamePacketListenerImpl.tryPickItem` for the acting player (the host's own, or a
    /// guest swapped in): picks `stack` into the 36 main slots, `infinite` with infinite
    /// materials (creative). Returns whether anything changed.
    pub(in crate::game) fn pick_into_inventory(
        &mut self,
        stack: &ItemStack,
        infinite: bool,
    ) -> bool {
        let picked = myriade_engine::pick_item::plan(
            &self.inventory[..36],
            self.selected,
            stack,
            infinite,
            &mut |held, _| myriade_engine::pick_item::vanilla_not_replaceable(held),
        );
        let Some(picked) = picked else {
            return false;
        };
        myriade_engine::pick_item::apply(
            &mut self.inventory[..36],
            &mut self.selected,
            stack,
            picked,
        );
        true
    }
}
