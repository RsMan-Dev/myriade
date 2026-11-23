//! The menus a LAN guest has open that the game runs for them, kept outside the guest's
//! `GuestPlay`. A guest's turn (`as_guest`) lifts the whole `GuestPlay` out of the host while it
//! runs, so a menu owner kept there is out of reach of anything that runs inside the turn: another
//! nested turn, a hook, an action that needs to find the menu to admit or undo. These owners stay
//! in the host, by connection, for as long as the guest is here.
use super::{
    lan::LanHost, lan_crafting::GuestCrafting, lan_stations::GuestStation,
    transactions::decision::MenuOwner,
};

/// One guest's menu owners: the inventory menu, the crafting table and the furnace or brewing
/// stand (`GuestCrafting`), and the workstation. Each owner counts every access, so an action can
/// tell that its menu was touched.
#[derive(Default)]
pub(in crate::game) struct GuestMenus {
    /// The inventory menu, the crafting table and the machine menu.
    pub(in crate::game) crafting: GuestCrafting,
    /// The workstation menu the guest has open (game/network/lan_stations.rs).
    pub(in crate::game) station: MenuOwner<Option<GuestStation>>,
}

/// What a mount's inventory is called, as vanilla's `HorseInventoryMenu` has no menu type.
const MOUNT_MENU: &str = "horizon:mount_inventory";

impl LanHost {
    /// Forget `conn`'s menu owners: the guest left or was removed, so nothing can name them again.
    pub(in crate::game) fn forget_menus(&mut self, conn: u32) -> Option<GuestMenus> {
        self.menus.remove(&conn)
    }
}

/// The menu `conn` has open, whichever way the game keeps it: a block's container, a trade, a
/// crafting table, a workstation, a lectern or a mount's inventory. Not the player's own inventory
/// menu. Its window id, its menu name and its block position when it has one.
pub(super) fn open_menu_of(host: &LanHost, conn: u32) -> Option<(i32, &str, Option<[i32; 3]>)> {
    let g = host.hub.guest(conn)?;
    let play = host.play.get(&conn);
    let menus = host.menus.get(&conn);
    if let Some(m) = &g.menu {
        let menu = host.hub.registry_name("minecraft:menu", m.kind)?;
        // A vehicle's menu is keyed by a stand-in, not a block.
        let pos = m
            .pos
            .filter(|p| p.1 != myriade_engine::lan::menu::VEHICLE_KEY_Y)
            .map(|p| [p.0, p.1, p.2]);
        return Some((m.window, menu, pos));
    }
    let play = play?;
    let (window, menu, pos) = if let Some(gm) = &play.merchant {
        (gm.window, "minecraft:merchant", None)
    } else if let Some((window, _, _, pos)) = menus.and_then(|m| m.crafting.table.as_ref()) {
        (*window, "minecraft:crafting", Some(*pos))
    } else if let Some(st) = menus.and_then(|m| m.station.as_ref()) {
        (
            st.window,
            super::lan_stations::menu_of(st.kind)?.0,
            Some(st.pos),
        )
    } else if let Some(l) = &play.lectern {
        (
            l.window,
            "minecraft:lectern",
            Some([l.key.0, l.key.1, l.key.2]),
        )
    } else if let Some(m) = &play.mount_window {
        (m.window, MOUNT_MENU, None)
    } else {
        return None;
    };
    Some((window, menu, pos))
}
