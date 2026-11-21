use super::*;

/// The hook points. Each does nothing in Horizon.
impl Game<'_> {
    /// Block `block` at `pos` opens its mod menu type `menu` for server
    /// player `player`: set `data` to hand the menu's screen bytes of your
    /// own (a machine's tier, mode or recipes), up to 32 KiB, which the
    /// client reads with `screen_extra_data` while the screen is open
    /// (NeoForge's `openMenu` with an extra-data writer). The first mod to
    /// set it wins, so answer only for your blocks. The player's state is not
    /// swapped in. 401.
    pub(in crate::game) fn native_menu_extra_data(
        &mut self,
        player: u128,
        menu: &str,
        block: &str,
        pos: [i32; 3],
        data: &mut Option<Vec<u8>>,
    ) {
        self.fire_menu_extra_data(player, menu, block, pos, data);
    }
}
