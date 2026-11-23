//! The horse inventory for a server's players (`ServerPlayer.openHorseInventory`):
//! shift + use on a tamed horse, donkey, mule, camel, llama or nautilus, or the
//! inventory key while riding one (`ServerboundPlayerCommandPacket`
//! OPEN_INVENTORY), opens `HorseInventoryMenu` over the mount with
//! `ClientboundMountScreenOpenPacket`. Clicks run on the menu engine
//! (`Kind::Mount`) with the mount's saddle, body item and chest copied in and
//! written back, and the slots go to the client after each change, as
//! `broadcastChanges` does.
use super::{Game, lan_actions::guest_within_entity_range, lan_crafting::input};
use myriade_engine::{
    items::ItemStack,
    menu::{Kind, Menu},
};
use horizon_protocol::packets::play::{self, Clientbound as C};

/// A guest's open mount menu.
pub(super) struct GuestMount {
    pub(super) window: i32,
    pub(super) mob: u32,
    columns: usize,
    state: i32,
    /// The menu's slots and the carried stack the client was last sent.
    sent: Option<(Vec<Option<ItemStack>>, Option<ItemStack>)>,
}

impl Game<'_> {
    /// `ServerPlayer.openHorseInventory` for mount `mob`: the open menu
    /// closes, the client gets the mount screen and the menu's slots.
    pub(super) fn lan_open_mount(&mut self, conn: u32, mob: u32) {
        let Some(columns) = self.mount_columns_of(mob) else {
            return;
        };
        if !self.guest_may_open(conn, "minecraft:horse_inventory", None) {
            return;
        }
        // ServerPlayer.openMenu closes the menu that was open.
        self.guest_close_all(conn);
        let Some(host) = self.lan.as_mut() else {
            return;
        };
        let Some(window) = host.hub.open_mount_window(conn, columns as i32, mob as i32) else {
            return;
        };
        host.play.entry(conn).or_default().mount_window = Some(GuestMount {
            window,
            mob,
            columns,
            state: 0,
            sent: None,
        });
        self.lan_send_mount(conn);
    }

    /// `HorseInventoryMenu` over mount `mob`, with the player's inventory
    /// loaded by the caller.
    fn mount_engine_menu(&self, mob: u32, columns: usize) -> Option<Menu> {
        let env = self.menu_env();
        let mut menu = Menu::new(Kind::Mount(columns));
        menu.set_registry_id(Some("horizon:mount_inventory"));
        menu.mount = Some(self.mount_kind_of(mob)?);
        let (saddle, body, items) = self.mount_contents_of(mob)?;
        menu.load_slot(0, saddle);
        menu.load_slot(1, body.flatten());
        for (k, item) in items.into_iter().take(columns * 3).enumerate() {
            menu.set_slot(&env, 2 + k, item);
        }
        Some(menu)
    }

    /// The menu's slots (the mount's, then the inventory and the hotbar)
    /// and the carried stack, sent when they changed since the last time.
    fn lan_send_mount(&mut self, conn: u32) {
        let Some(host) = self.lan.as_ref() else {
            return;
        };
        let Some(gm) = host.play.get(&conn).and_then(|p| p.mount_window.as_ref()) else {
            return;
        };
        let (mob, columns) = (gm.mob, gm.columns);
        if !self.mob_exists(mob) {
            return;
        }
        let Some(g) = host.hub.guests.iter().find(|g| g.conn == conn) else {
            return;
        };
        let Some((saddle, body, items)) = self.mount_contents_of(mob) else {
            return;
        };
        let mut slots = vec![saddle, body.flatten()];
        slots.extend(items.into_iter().take(columns * 3));
        slots.extend((9..36).chain(0..9).map(|i| g.inventory[i].clone()));
        let carried = g.carried.clone();
        let now = Some((slots, carried));
        let Some(host) = self.lan.as_mut() else {
            return;
        };
        let Some(gm) = host
            .play
            .get_mut(&conn)
            .and_then(|p| p.mount_window.as_mut())
        else {
            return;
        };
        if gm.sent == now {
            return;
        }
        gm.state = (gm.state + 1) & 0x7FFF;
        let (window, state) = (gm.window, gm.state);
        let (slots, carried) = now.clone().unwrap_or_default();
        gm.sent = now;
        let content = C::ContainerSetContent(play::ContainerSetContent {
            container: window,
            state,
            items: slots.iter().map(|s| host.hub.stack(s)).collect(),
            carried: host.hub.stack(&carried),
        });
        host.hub.send_to(conn, &content);
    }

    /// A click on the guest's mount window; false for other windows.
    pub(super) fn lan_mount_click(&mut self, conn: u32, p: &play::ContainerClick) -> bool {
        let Some(host) = self.lan.as_ref() else {
            return false;
        };
        let Some((mob, columns)) = host
            .play
            .get(&conn)
            .and_then(|pl| pl.mount_window.as_ref())
            .filter(|m| m.window == p.container)
            .map(|m| (m.mob, m.columns))
        else {
            return false;
        };
        // handleContainerClick: a menu that is no longer `stillValid` takes no click. A mount
        // that died this tick has dropped its inventory already while its body still holds it;
        // the menu itself closes in `tick_lan_mount_menus`.
        if let Some(gi) = host.hub.guests.iter().position(|g| g.conn == conn)
            && !host.hub.guests[gi].spectator()
            && self.lan_mount_valid(&host.hub.guests[gi], mob)
            && self.mob_exists(mob)
            && let Some(mut menu) = self.mount_engine_menu(mob, columns)
        {
            self.guest_menu_op(gi, &mut menu, |menu, env| {
                let _ = menu.click(env, i32::from(p.slot), i32::from(p.button), input(p.input));
            });
            self.lan_mount_store(mob, columns, &menu);
        }
        // The client is told what the server has, whatever it predicted.
        let Some(host) = self.lan.as_mut() else {
            return true;
        };
        if let Some(gm) = host
            .play
            .get_mut(&conn)
            .and_then(|pl| pl.mount_window.as_mut())
        {
            gm.sent = None;
        }
        self.lan_send_mount(conn);
        true
    }

    /// The engine menu's mount slots back on mount `mob`.
    fn lan_store_slots(menu: &Menu, columns: usize) -> Vec<Option<ItemStack>> {
        (0..columns * 3)
            .map(|k| menu.slot(2 + k).cloned())
            .collect()
    }
    fn lan_mount_store(&mut self, mob: u32, columns: usize, menu: &Menu) {
        let Some((_, body, _)) = self.mount_contents_of(mob) else {
            return;
        };
        let body = body.map(|_| menu.slot(1).cloned());
        let items = Self::lan_store_slots(menu, columns);
        self.mount_store_of(mob, menu.slot(0).cloned(), body, items);
    }

    /// `AbstractContainerMenu.removed` of the guest's mount menu (`window`;
    /// None closes it whichever it is): the carried stack goes back to the
    /// inventory. False when no such window is open.
    pub(super) fn lan_mount_closed(&mut self, conn: u32, window: Option<i32>) -> bool {
        let Some(host) = self.lan.as_mut() else {
            return false;
        };
        let closed = host
            .play
            .get_mut(&conn)
            .and_then(|p| {
                p.mount_window
                    .take_if(|m| window.is_none_or(|w| m.window == w))
            })
            .is_some();
        if closed
            && let Some(gi) = host.hub.guests.iter().position(|g| g.conn == conn)
            && let Some(carried) = host.hub.guests.update(gi, |g| g.carried.take())
        {
            self.lan_place_back(gi, vec![carried]);
        }
        closed
    }

    /// `AbstractMountInventoryMenu.stillValid`: the mount is alive and the
    /// guest's eyes are within the entity interaction range (3, 5 in creative)
    /// plus 4 of its box (`Player.isWithinEntityInteractionRange(mount, 4.0)`).
    pub(super) fn lan_mount_valid(&self, g: &myriade_engine::lan::guest::Guest, mob: u32) -> bool {
        self.mob_alive(mob) && self.mob_box(mob).is_some_and(|b| guest_within_entity_range(g, &b, 4.0))
    }

    /// Each tick: a mount menu whose mount died or is out of reach closes
    /// (`AbstractMountInventoryMenu.stillValid`: alive, within the entity
    /// interaction range plus 4 of the eyes), and the others follow the
    /// mount's and the inventory's slots. Only the level the player is in
    /// checks its menu (`ServerPlayer.tick`): another level ticking has
    /// none of its mobs (#5331).
    pub(super) fn tick_lan_mount_menus(&mut self) {
        let Some(host) = self.lan.as_ref() else {
            return;
        };
        let open: Vec<(u32, u32, i32)> = host
            .play
            .iter()
            .filter_map(|(c, p)| p.mount_window.as_ref().map(|m| (*c, m.mob, m.window)))
            .collect();
        for (conn, mob, window) in open {
            let Some(host) = self.lan.as_ref() else {
                return;
            };
            let Some(g) = host.hub.guests.iter().find(|g| g.conn == conn) else {
                continue;
            };
            if !host.hub.here(g) {
                continue;
            }
            if self.lan_mount_valid(g, mob) {
                self.lan_send_mount(conn);
            } else {
                self.lan_mount_closed(conn, Some(window));
                if let Some(host) = self.lan.as_mut() {
                    host.hub.send_to(
                        conn,
                        &C::ContainerClose(play::ContainerClose { container: window }),
                    );
                }
            }
        }
    }
}
