//! The creative toolbars, `hotbar.nbt` beside `options.txt`: load or save the
//! hotbar row of a hotbar key (`Minecraft.handleKeybinds`).
use super::Game;
use horizon_client::hotbar;

impl Game<'_> {
    /// `CreativeModeInventoryScreen.handleHotbarLoadOrSave` for the hotbar key
    /// `index`: load puts the saved row in the hotbar (the creative slot
    /// packets follow from the inventory change), save stores the hotbar.
    pub(super) fn hotbar_load_or_save(&mut self, index: usize, load: bool) {
        let path = Self::options_path().with_file_name("hotbar.nbt");
        let known = |id: &str| self.items().get(id).is_some();
        let mut rows = hotbar::load(&path, &known);
        if load {
            for (slot, stack) in rows[index].iter().enumerate() {
                self.inventory[slot] = stack.clone();
            }
            return;
        }
        rows[index] = self.inventory[..hotbar::SIZE].to_vec();
        if let Err(e) = hotbar::save(&path, &rows) {
            // saveHotbar sets the message inside its try, so a failed save says nothing.
            log::warn!("the creative hotbar was not saved: {e}");
        }
    }
}
