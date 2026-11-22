//! `BundleMouseActions`: the wheel over a bundle in a container screen selects
//! one of the items it shows, which a click that takes one out of it then takes
//! (`BundleContents.Mutable.removeOne`). Moving off the bundle clears the
//! selection. On a server the selection is sent to it (`ServerboundSelectBundleItemPacket`).
use super::Game;
use horizon_client::bundle::toggle_selected;

impl Game<'_> {
    /// A slot a bundle can be selected in: the player's inventory and off hand in any
    /// menu (#4436: a crafting table's, an enchanting table's...), and a container's own
    /// slots. A server only knows those in chest-like menus.
    fn bundle_slot(&mut self, slot: usize) -> bool {
        slot < 36 || slot == 504 || ((300..500).contains(&slot) && self.slots().contains(&slot))
    }


    /// `BundleMouseActions.toggleSelectedBundleItem`: the stack here and, on a server, there.
    fn select_bundle(&mut self, slot: usize, index: i32) {
        if let Some(stack) = self.slot_mut(slot) {
            toggle_selected(stack, index);
        }
    }

    /// `BundleMouseActions.unselectedBundleItem` for the bundle in `slot`, if it has a selection.
    pub(super) fn unselect_bundle(&mut self, slot: usize) {
        if !self.bundle_slot(slot) {
            return;
        }
        let selected = self
            .slot_mut(slot)
            .as_ref()
            .and_then(|s| s.data())
            .is_some_and(|d| d.bundle_selected.0 >= 0);
        if selected {
            self.select_bundle(slot, -1);
        }
    }
}
