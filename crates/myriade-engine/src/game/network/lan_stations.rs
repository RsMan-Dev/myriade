//! Workstation menus for LAN guests: the screen a guest's use opens runs on
//! the host's menu engine (`myriade_engine::menu`), the one single player
//! uses, with vanilla's packets: OpenScreen with the station's `MenuType`,
//! container clicks, `ServerboundContainerButtonClickPacket`, the content and
//! data slot sync after each (`broadcastChanges`), and the inputs back into
//! the inventory when the menu closes (`AbstractContainerMenu.clearContainer`).
use super::{
    Game,
    lan_crafting::input,
    stations::{Kind as StationKind, Station},
};
use myriade_engine::{
    containers::BlockEntity,
    items::ItemStack,
    menu::{AnvilCraft, Kind, Menu},
};
use horizon_core::random::RandomSource;
use horizon_protocol::packets::play::{self, Clientbound as C};

/// A guest's open workstation menu.
#[derive(Clone)]
pub(super) struct GuestStation {
    pub(super) window: i32,
    state: i32,
    pub(super) kind: StationKind,
    pub(super) pos: [i32; 3],
    pub(super) menu: Box<Menu>,
    /// The data slots the client was last sent. `None` is not sent yet,
    /// which is not a cost: an anvil's signed cost may be `i32::MIN`.
    sent_data: Vec<Option<i32>>,
    /// The game time of the menu's last sound (`lastSoundTime`).
    last_sound: u64,
}

/// A guest's open lectern: `LecternMenu` over the block entity, made on the
/// menu engine for each click (`Game::lectern_menu`).
pub(super) struct GuestLectern {
    pub(super) window: i32,
    state: i32,
    pub(super) key: (i32, i32, i32),
    /// The book and page the client was last sent (`broadcastChanges`).
    sent: Option<(Option<ItemStack>, i32)>,
}

/// The `MenuType`, the engine's menu and the input slots
/// (`clearContainer`'s container) of a station a guest can use.
pub(super) fn menu_of(kind: StationKind) -> Option<(&'static str, Kind, std::ops::Range<usize>)> {
    Some(match kind {
        StationKind::Loom => ("minecraft:loom", Kind::Loom, 0..3),
        StationKind::Stonecutter => ("minecraft:stonecutter", Kind::Stonecutter, 0..1),
        StationKind::Grindstone => ("minecraft:grindstone", Kind::Grindstone, 0..2),
        StationKind::Anvil => ("minecraft:anvil", Kind::Anvil, 0..2),
        StationKind::Smithing => ("minecraft:smithing", Kind::Smithing, 0..3),
        StationKind::Enchanting => ("minecraft:enchantment", Kind::Enchanting, 0..2),
        StationKind::Beacon => ("minecraft:beacon", Kind::Beacon, 0..1),
        StationKind::Cartography => ("minecraft:cartography_table", Kind::Cartography, 0..2),
        _ => return None,
    })
}

impl Game<'_> {
    /// A workstation a guest's use opened (`ServerPlayer.openMenu`); false
    /// when guests can't use that one yet.
    pub(super) fn lan_open_station(&mut self, conn: u32, station: &Station) -> bool {
        self.lan_open_station_at(conn, station.kind, station.pos, Vec::new())
            .is_some()
    }

    /// `lan_open_station` of a station `kind` at `pos`, its input slots starting with `inputs`
    /// (a mod's station with no block, game/modding/native_stations.rs): None when guests can't
    /// use that kind, Some(None) when a mod stopped it or no window opened, else its window.
    pub(super) fn lan_open_station_at(
        &mut self,
        conn: u32,
        kind: StationKind,
        pos: [i32; 3],
        inputs: Vec<Option<ItemStack>>,
    ) -> Option<Option<i32>> {
        let station_kind = kind;
        let (menu_type, kind, input_slots) = menu_of(station_kind)?;
        // The mods may stop the menu; the click is spent.
        if !self.guest_may_open(conn, menu_type, Some(pos)) {
            return Some(None);
        }
        // ServerPlayer.openMenu closes the menu that was open.
        self.guest_close_all(conn);
        let title = horizon_protocol::text::translatable(station_kind.title_key(), &[]);
        let Some(window) = self
            .lan
            .as_mut()
            .and_then(|host| host.hub.open_window(conn, menu_type, title))
        else {
            return Some(None);
        };
        let mut menu = Box::new(Menu::new(kind));
        menu.set_registry_id(Some(menu_type));
        if kind == Kind::Enchanting {
            // Player.enchantmentSeed, made when first needed.
            let needs_seed = self
                .lan
                .as_ref()
                .and_then(|host| host.hub.guests.iter().find(|g| g.conn == conn))
                .is_some_and(|g| g.xp_seed == 0);
            let drawn = needs_seed.then(|| self.world.random.next_int());
            if let Some(host) = self.lan.as_mut() {
                let _ = host.hub.update_guest(conn, |g| {
                    if let Some(drawn) = drawn {
                        g.xp_seed = drawn;
                    }
                    menu.data[3] = g.xp_seed;
                });
            }
        }
        if kind == Kind::Beacon {
            menu.data = self.beacon_data(pos).to_vec();
        }
        if !inputs.is_empty()
            && let Some(gi) = self
                .lan
                .as_ref()
                .and_then(|host| host.hub.guests.iter().position(|g| g.conn == conn))
        {
            self.guest_menu_op(gi, &mut menu, |menu, env| {
                for (slot, stack) in input_slots.zip(inputs) {
                    menu.set_slot(env, slot, stack);
                }
            });
            self.lan_station_created(&mut menu, station_kind, true);
        }
        if let Some(host) = self.lan.as_mut() {
            *host.menus.entry(conn).or_default().station = Some(GuestStation {
                window,
                state: 0,
                kind: station_kind,
                pos,
                menu,
                sent_data: Vec::new(),
                last_sound: 0,
            });
        }
        self.lan_send_station(conn);
        Some(Some(window))
    }

    /// The smithing result once createResult ran on a click that changed the
    /// inputs, set again in the menu as it stands.
    fn lan_station_created(&mut self, menu: &mut Menu, kind: StationKind, recomputed: bool) {
        if recomputed && kind == StationKind::Smithing {
            let env = self.menu_env();
            let result = menu.slot(3).cloned();
            menu.set_slot(&env, 3, result);
        }
    }

    /// Runs `op` on the guest's open station (window `window`, any when
    /// None) with their inventory in it, then sends what changed. False when
    /// no such window is open. The station is out of the guest's menus while the mods are asked
    /// (`anvil_update`, `anvil_craft_pre`...), so it is busy: a hook can't open another in its
    /// place.
    fn lan_station_op(
        &mut self,
        conn: u32,
        window: Option<i32>,
        op: impl FnOnce(&mut Menu, &myriade_engine::menu::Env),
    ) -> bool {
        self.lan_station_op_busy(conn, window, op)
    }

    fn lan_station_op_busy(
        &mut self,
        conn: u32,
        window: Option<i32>,
        op: impl FnOnce(&mut Menu, &myriade_engine::menu::Env),
    ) -> bool {
        let Some(mut gs) = self
            .lan
            .as_mut()
            .and_then(|host| host.menus.get_mut(&conn))
            .and_then(|p| p.station.take_if(|s| window.is_none_or(|w| s.window == w)))
        else {
            return false;
        };
        let guest = self
            .lan
            .as_ref()
            .and_then(|host| host.hub.guests.iter().position(|g| g.conn == conn))
            .filter(|&gi| {
                !self
                    .lan
                    .as_ref()
                    .is_some_and(|h| h.hub.guests[gi].spectator())
            });
        if let Some(gi) = guest {
            // createResult runs when the inputs change (or a rename): a click
            // elsewhere must not hand the hooks their own result again.
            let result_slot = match gs.kind {
                StationKind::Anvil | StationKind::Grindstone => Some(2),
                StationKind::Smithing => Some(3),
                _ => None,
            };
            let before = result_slot.map(|r| {
                let slots: Vec<_> = (0..=r).map(|s| gs.menu.slot(s).cloned()).collect();
                // Only the anvil has a data slot (its cost) and a name; grindstone and
                // smithing menus have neither. A rename runs createResult even when the
                // result comes out the same (AnvilMenu.setItemName).
                (
                    slots,
                    gs.menu.data.first().copied(),
                    gs.menu.anvil.name.clone(),
                )
            });
            self.guest_menu_op(gi, &mut gs.menu, op);
            // AnvilCraftEvent.Pre has no listener: the taken result is final.
            let anvil_craft = gs.menu.anvil_craft.take();
            if anvil_craft.is_some() {
                gs.menu.anvil_take(&self.menu_env());
                let dropped = std::mem::take(&mut gs.menu.dropped);
                self.drop_guest_items(gi, dropped);
            }
            let mut ground_xp = None;
            if let Some(take) = gs.menu.grind_take.take() {
                let env = self.menu_env();
                ground_xp = gs.menu.grindstone_taken(&env, take, false);
            }
            let recomputed = before.is_some_and(|(slots, cost, item_name)| {
                cost != gs.menu.data.first().copied()
                    || item_name != gs.menu.anvil.name
                    || slots
                        .iter()
                        .enumerate()
                        .any(|(s, b)| gs.menu.slot(s) != b.as_ref())
            });
            self.lan_station_created(&mut gs.menu, gs.kind, recomputed);
            self.lan_station_sounds(&mut gs);
            if let Some(amount) = gs.menu.ground.take() {
                self.grindstone_taken(gs.pos, amount);
            }
            if let Some(xp) = ground_xp {
                self.grindstone_award(gs.pos, xp.unsigned_abs());
            }
            self.lan_station_combined(conn, &mut gs);
            if let Some(button) = gs.menu.enchant_button.take() {
                self.lan_enchant(gi, &mut gs, button);
            }
            if std::mem::take(&mut gs.menu.enchant_changed) {
                self.lan_enchant_offers(gi, &mut gs);
            }
        }
        if let Some(host) = self.lan.as_mut() {
            *host.menus.entry(conn).or_default().station = Some(gs);
        }
        self.lan_send_station(conn);
        true
    }

    /// How many slots of its own the guest's open station menu has before the
    /// player's inventory (`slot_count - 36`), if one is open.
    pub(super) fn lan_station_own_slots(&self, conn: u32) -> Option<usize> {
        let gs = self.lan.as_ref()?.menus.get(&conn)?.station.as_ref()?;
        Some(gs.menu.slot_count() - 36)
    }

    /// The lectern's book and page for the client: everything (`all`), or
    /// what changed since it was last sent.
    fn lan_send_lectern(&mut self, conn: u32, all: bool) {
        let Some((key, window, sent)) = self
            .lan
            .as_ref()
            .and_then(|host| host.play.get(&conn))
            .and_then(|p| p.lectern.as_ref())
            .map(|gl| (gl.key, gl.window, &gl.sent))
        else {
            return;
        };
        let (book, page) = match self.world.block_entities.get(&key) {
            Some(BlockEntity::Lectern(l)) => (&l.book, l.page),
            _ => (&None, 0),
        };
        if !all
            && sent
                .as_ref()
                .is_some_and(|(old, p)| old == book && *p == page)
        {
            return;
        }
        let page_changed = sent.as_ref().is_none_or(|(_, p)| *p != page);
        let book = book.clone();
        let now = Some((book.clone(), page));
        if let Some(host) = self.lan.as_mut() {
            if let Some(gl) = host.play.get_mut(&conn).and_then(|p| p.lectern.as_mut()) {
                gl.sent = now;
                gl.state = (gl.state + 1) & 0x7FFF;
            }
            let state = host
                .play
                .get(&conn)
                .and_then(|p| p.lectern.as_ref())
                .map_or(0, |gl| gl.state);
            let carried = host
                .hub
                .guests
                .iter()
                .find(|g| g.conn == conn)
                .and_then(|g| g.carried.clone());
            let content = C::ContainerSetContent(play::ContainerSetContent {
                container: window,
                state,
                items: vec![host.hub.stack(&book)],
                carried: host.hub.stack(&carried),
            });
            host.hub.send_to(conn, &content);
            if all || page_changed {
                host.hub.send_to(
                    conn,
                    &C::ContainerSetData(play::ContainerSetData {
                        container: window,
                        id: 0,
                        value: page as i16,
                    }),
                );
            }
        }
    }

    /// Runs `op` on the guest's lectern menu (window `window`) with their
    /// inventory, the block entity takes the book and page back, and the
    /// client gets what changed. False when no such window is open.
    fn lan_lectern_op(
        &mut self,
        conn: u32,
        window: i32,
        op: impl FnOnce(&mut Menu, &myriade_engine::menu::Env),
    ) -> bool {
        let Some(key) = self
            .lan
            .as_ref()
            .and_then(|host| host.play.get(&conn))
            .and_then(|p| p.lectern.as_ref())
            .filter(|l| l.window == window)
            .map(|l| l.key)
        else {
            return false;
        };
        let guest = self
            .lan
            .as_ref()
            .and_then(|host| host.hub.guests.iter().position(|g| g.conn == conn))
            .filter(|&gi| {
                !self
                    .lan
                    .as_ref()
                    .is_some_and(|h| h.hub.guests[gi].spectator())
            });
        if let Some(gi) = guest
            && let Some(mut menu) = self.lectern_menu(key)
        {
            // Player.mayBuild: survival and creative.
            let may_build = self
                .lan
                .as_ref()
                .is_some_and(|host| matches!(host.hub.guests[gi].game_mode, 0 | 1));
            self.guest_menu_op(gi, &mut menu, |menu, env| {
                menu.may_build = may_build;
                op(menu, env);
            });
            self.lectern_menu_done(key, &menu);
        }
        self.lan_send_lectern(conn, true);
        true
    }

    /// A click on the guest's lectern window; false for other windows.
    pub(super) fn lan_lectern_click(&mut self, conn: u32, p: &play::ContainerClick) -> bool {
        self.lan_lectern_op(conn, p.container, |menu, env| {
            let _ = menu.click(env, i32::from(p.slot), i32::from(p.button), input(p.input));
        })
    }

    /// The rest of a taken anvil or smithing result's `onTake`: the guest
    /// pays the levels, the anvil wears (level event 1030 or 1029), the
    /// smithing table sounds (1044).
    fn lan_station_combined(&mut self, conn: u32, gs: &mut GuestStation) {
        let change = std::mem::take(&mut gs.menu.level_change);
        if change != 0 {
            self.as_guest(conn, |g| g.give_levels(change));
        }
        if !std::mem::take(&mut gs.menu.combined) {
            return;
        }
        if gs.kind == StationKind::Smithing {
            self.level_event_at(1044, gs.pos, 0);
        } else {
            let creative = gs.menu.creative;
            // A broken anvil closes the menu on the next stillValid check.
            self.anvil_used(gs.pos, creative);
        }
    }

    /// `EnchantmentMenu.slotsChanged` with a level: the costs and clues for
    /// the guest's seed and the bookshelves around the table.
    fn lan_enchant_offers(&mut self, gi: usize, gs: &mut GuestStation) {
        let seed = self
            .lan
            .as_ref()
            .map_or(0, |host| host.hub.guests[gi].xp_seed);
        let bookcases = self.count_bookshelves(gs.pos);
        let offers = myriade_engine::stations::enchant_offers(
            &self.stations().enchantments,
            self.items(),
            seed,
            bookcases,
            gs.menu.slot(0),
        );
        let (costs, clues) = (offers.costs, offers.clues);
        for i in 0..3 {
            gs.menu.data[i] = costs[i];
            let clue = clues[i].as_ref();
            gs.menu.data[4 + i] = clue
                .and_then(|(e, _)| {
                    self.lan
                        .as_ref()
                        .and_then(|host| host.hub.registry_id("minecraft:enchantment", e))
                })
                .unwrap_or(-1);
            gs.menu.data[7 + i] = clue.map_or(-1, |(_, l)| *l as i32);
        }
        gs.menu.data[3] = seed;
    }

    /// `EnchantmentMenu.clickMenuButton`'s `access.execute`: the item gets
    /// the offer's enchantments, the guest pays its levels and lapis and
    /// gets a new seed, the table sounds, and the offers are made again.
    fn lan_enchant(&mut self, gi: usize, gs: &mut GuestStation, button: usize) {
        let Some(item) = gs.menu.slot(0).cloned() else {
            return;
        };
        let Some((seed, creative, conn)) = self.lan.as_ref().map(|host| {
            let g = &host.hub.guests[gi];
            (g.xp_seed, g.creative(), g.conn)
        }) else {
            return;
        };
        let Some(enchanted) = self.table_enchant(seed, button, gs.menu.data[button], &item) else {
            return;
        };
        let need = button as i32 + 1;
        // Player.onEnchantmentPerformed, and EnchantmentMenu's awardStat(ENCHANT_ITEM).
        self.as_guest(conn, |game| {
            game.enchantment_performed(need);
            game.award_custom_stat("enchant_item", 1);
            // CriteriaTriggers.ENCHANTED_ITEM, with the levels it cost.
            game.trigger_enchanted_item(&enchanted, need);
        });
        let new_seed = self.world.random.next_int();
        if let Some(host) = self.lan.as_mut() {
            {
                let value = new_seed;
                host.hub.guests.update(gi, |guest| guest.xp_seed = value);
            }
        }
        let env = self.menu_env();
        let lapis = gs.menu.slot(1).cloned().filter(|_| !creative).map(|mut l| {
            l.count = l.count.saturating_sub(need as u32);
            l
        });
        gs.menu.set_slot(&env, 0, Some(enchanted));
        if !creative {
            gs.menu.set_slot(&env, 1, lapis.filter(|l| l.count > 0));
        }
        gs.menu.enchant_changed = false;
        self.lan_enchant_offers(gi, gs);
        let pitch = self.world.random.next_float() * 0.1 + 0.9;
        let at = gs.pos.map(|v| f64::from(v) + 0.5);
        self.audio.play(
            "minecraft:block.enchantment_table.use",
            Some(at),
            1.0,
            pitch,
        );
    }

    /// `ServerGamePacketListenerImpl.handleRenameItem`: the guest's anvil
    /// takes the name (`AnvilMenu.setItemName`).
    pub(super) fn lan_station_rename(&mut self, conn: u32, name: &str) {
        let anvil = self
            .lan
            .as_ref()
            .and_then(|host| host.menus.get(&conn))
            .and_then(|p| p.station.as_ref())
            .is_some_and(|s| s.kind == StationKind::Anvil);
        if anvil
            && self.chat_filter_text(
                conn,
                &[name],
                myriade_engine::lan::hub::chat_filter::Place::Anvil,
            )
        {
            self.lan_station_op(conn, None, |menu, env| {
                menu.rename(env, name);
            });
        }
    }

    /// The sounds the menu played, at the station (heard by everyone around,
    /// as `level.playSound(null, pos, ...)`), once per tick.
    fn lan_station_sounds(&mut self, gs: &mut GuestStation) {
        let sounds = std::mem::take(&mut gs.menu.sounds);
        if sounds.is_empty() || gs.last_sound == self.ticks {
            return;
        }
        gs.last_sound = self.ticks;
        let at = gs.pos.map(|v| f64::from(v) + 0.5);
        // The menu's `lastSoundTime` lets one sound through per tick, however
        // many results a shift-click took.
        self.audio.play(sounds[0], Some(at), 1.0, 1.0);
    }

    /// A click on the guest's station window; false for other windows.
    pub(super) fn lan_station_click(&mut self, conn: u32, p: &play::ContainerClick) -> bool {
        self.lan_station_op(conn, Some(p.container), |menu, env| {
            let _ = menu.click(env, i32::from(p.slot), i32::from(p.button), input(p.input));
        })
    }

    /// `ServerGamePacketListenerImpl.handleContainerButtonClick`:
    /// `clickMenuButton`, then `broadcastChanges` when the menu took it.
    pub(super) fn lan_station_button(&mut self, conn: u32, window: i32, id: i32) {
        // LecternMenu's page buttons and Take Book.
        if self.lan_lectern_op(conn, window, |menu, env| {
            menu.button(env, id);
        }) {
            return;
        }
        self.lan_station_op(conn, Some(window), |menu, env| {
            menu.button(env, id);
        });
    }

    /// The whole station menu and its changed data slots for the client
    /// (`sendAllDataToRemote` / `broadcastChanges`).
    fn lan_send_station(&mut self, conn: u32) {
        let Some(host) = self.lan.as_mut() else {
            return;
        };
        let Some(gi) = host.hub.guests.iter().position(|g| g.conn == conn) else {
            return;
        };
        let Some(gs) = host.menus.get_mut(&conn).and_then(|p| p.station.as_mut()) else {
            return;
        };
        gs.state = (gs.state + 1) & 0x7FFF;
        let own = gs.menu.slot_count() - 36;
        let mut items: Vec<Option<ItemStack>> =
            (0..own).map(|s| gs.menu.slot(s).cloned()).collect();
        let g = &host.hub.guests[gi];
        items.extend((9..36).chain(0..9).map(|i| g.inventory[i].clone()));
        let carried = g.carried.clone();
        let packet = C::ContainerSetContent(play::ContainerSetContent {
            container: gs.window,
            state: gs.state,
            items: items.iter().map(|s| host.hub.stack(s)).collect(),
            carried: host.hub.stack(&carried),
        });
        // ContainerSetData's value is a short. An anvil cost outside it
        // would change sign (`-65535` as `+1`), so the signed cost follows
        // on `horizon:anvil_cost` after that short.
        gs.sent_data.resize(gs.menu.data.len(), None);
        let mut data = Vec::new();
        let mut wide_cost = None;
        let anvil = gs.kind == StationKind::Anvil;
        let window = gs.window;
        for (k, &v) in gs.menu.data.iter().enumerate() {
            if gs.sent_data[k] == Some(v) {
                continue;
            }
            gs.sent_data[k] = Some(v);
            let mut value = v as i16;
            if anvil && k == 0 {
                let synced = horizon_protocol::anvil_cost::sync(v);
                value = synced.short;
                wide_cost = synced.wide;
            }
            data.push(C::ContainerSetData(play::ContainerSetData {
                container: window,
                id: k as i16,
                value,
            }));
        }
        host.hub.send_to(conn, &packet);
        host.hub.guests.update(gi, |g| g.inventory_told());
        for packet in data {
            host.hub.send_to(conn, &packet);
        }
        if let Some(cost) = wide_cost {
            host.hub.send_to(
                conn,
                &C::CustomPayload(horizon_protocol::packets::configuration::CustomPayload {
                    channel: horizon_protocol::anvil_cost::CHANNEL.into(),
                    data: horizon_protocol::anvil_cost::encode(
                        horizon_protocol::anvil_cost::Cost { window, cost },
                    ),
                }),
            );
        }
    }

    /// `AbstractContainerMenu.removed` of the guest's station (`window`;
    /// None closes it whichever it is): the carried stack and the inputs go
    /// back to the inventory, what doesn't fit is dropped. False when no such
    /// window is open.
    pub(super) fn lan_station_closed(&mut self, conn: u32, window: Option<i32>) -> bool {
        // A lectern's menu keeps nothing of its own (the book is the block entity's).
        let lectern = self
            .lan
            .as_mut()
            .and_then(|host| host.play.get_mut(&conn))
            .and_then(|p| p.lectern.take_if(|l| window.is_none_or(|w| l.window == w)))
            .is_some();
        if lectern {
            // AbstractContainerMenu.removed: the carried stack goes back.
            if let Some(gi) = self
                .lan
                .as_ref()
                .and_then(|host| host.hub.guests.iter().position(|g| g.conn == conn))
            {
                let carried = self
                    .lan
                    .as_mut()
                    .and_then(|host| host.hub.guests.update(gi, |g| g.carried.take()));
                if let Some(carried) = carried {
                    self.lan_place_back(gi, vec![carried]);
                }
            }
            if window.is_some() {
                return true;
            }
        }
        let Some(gs) = self
            .lan
            .as_mut()
            .and_then(|host| host.menus.get_mut(&conn))
            .and_then(|p| p.station.take_if(|s| window.is_none_or(|w| s.window == w)))
        else {
            return false;
        };
        let Some(gi) = self
            .lan
            .as_ref()
            .and_then(|host| host.hub.guests.iter().position(|g| g.conn == conn))
        else {
            return true;
        };
        // AbstractContainerMenu.removed: the carried stack first, then the inputs.
        let mut back: Vec<ItemStack> = self
            .lan
            .as_mut()
            .map(|host| host.hub.guests.update(gi, |g| g.carried.take()))
            .unwrap_or_default()
            .into_iter()
            .collect();
        let inputs = station_inputs(&gs);
        if gs.kind == StationKind::Beacon {
            // BeaconMenu.removed drops the payment (player.drop).
            if !back.is_empty() {
                self.lan_place_back(gi, std::mem::take(&mut back));
            }
            self.drop_guest_items(gi, inputs);
        } else {
            back.extend(inputs);
        }
        if !back.is_empty() {
            self.lan_place_back(gi, back);
        }
        true
    }

    /// `BeaconBlockEntity.dataAccess`: the pyramid's levels and the powers
    /// (`BeaconMenu.encodeEffect`: the `mob_effect` id + 1, 0 for none).
    fn beacon_data(&self, pos: [i32; 3]) -> [i32; 3] {
        let Some(BlockEntity::Beacon(b)) = self.world.block_entities.get(&(pos[0], pos[1], pos[2]))
        else {
            return [0; 3];
        };
        let encode = |e: &Option<String>| {
            e.as_deref()
                .and_then(|n| {
                    self.lan
                        .as_ref()
                        .and_then(|host| host.hub.registry_id("minecraft:mob_effect", n))
                })
                .map_or(0, |id| id + 1)
        };
        [b.levels, encode(&b.primary), encode(&b.secondary)]
    }

    /// `ServerGamePacketListenerImpl.handleSetBeaconPacket`: the guest's
    /// beacon menu takes the powers (`BeaconMenu.updateEffects`), and they
    /// become the block entity's (`dataAccess.set`: the power_select sound
    /// while the beam is up).
    pub(super) fn lan_station_beacon(
        &mut self,
        conn: u32,
        primary: Option<i32>,
        secondary: Option<i32>,
    ) {
        let Some(pos) = self
            .lan
            .as_ref()
            .and_then(|host| host.menus.get(&conn))
            .and_then(|p| p.station.as_ref())
            .filter(|s| s.kind == StationKind::Beacon)
            .map(|s| s.pos)
        else {
            return;
        };
        if !self
            .lan
            .as_ref()
            .is_some_and(|host| host.hub.guests.iter().any(|g| g.conn == conn))
        {
            return;
        }
        // An id the registry doesn't have fails the packet's codec.
        let mut powers: [Option<(i32, String)>; 2] = [None, None];
        for (k, id) in [primary, secondary].into_iter().enumerate() {
            if let Some(id) = id {
                let Some(name) = self
                    .lan
                    .as_ref()
                    .and_then(|host| host.hub.registry_name("minecraft:mob_effect", id))
                else {
                    return;
                };
                powers[k] = Some((id, name.to_owned()));
            }
        }
        let data = self.beacon_data(pos);
        let mut updated = None;
        self.lan_station_op(conn, None, |menu, env| {
            menu.data.copy_from_slice(&data);
            let power = |k: usize| powers[k].as_ref().map(|(id, n)| (*id, n.as_str()));
            updated = Some(menu.beacon_update(env, power(0), power(1)));
        });
        // handleSetBeaconPacket: effects the beacon can't give, or no payment, disconnect.
        if updated == Some(false)
            && let Some(host) = self.lan.as_mut()
        {
            let name = host
                .hub
                .guests
                .iter()
                .find(|g| g.conn == conn)
                .map(|g| g.name.clone());
            eprintln!(
                "Player {} tried to set invalid beacon effects",
                name.unwrap_or_default()
            );
            host.hub.kick(
                conn,
                horizon_protocol::text::translatable("multiplayer.disconnect.generic", &[]),
            );
        }
        if updated != Some(true) {
            return;
        }
        let [primary, secondary] = powers.map(|p| p.map(|(_, n)| n));
        let mut select_sound = false;
        if let Some(BlockEntity::Beacon(b)) =
            self.world.block_entities.get_mut(&(pos[0], pos[1], pos[2]))
        {
            b.primary = primary.clone();
            b.secondary = secondary.clone();
            select_sound = !b.sections.is_empty();
        }
        if select_sound {
            self.beacon_sound(pos, "minecraft:block.beacon.power_select");
        }
        self.world.modified.insert((pos[0] >> 4, pos[2] >> 4));
    }

    /// `ServerPlayer.doTick`'s `stillValid`: a guest's station closes once its
    /// block is gone or the guest is out of reach (`isWithinBlockInteractionRange(pos, 4)`).
    pub(super) fn lan_stations_tick(&mut self) {
        self.lan_lecterns_tick();
        let conns: Vec<u32> = self
            .lan
            .as_ref()
            .map(|host| {
                host.menus
                    .iter()
                    .filter(|(_, p)| p.station.is_some())
                    .map(|(c, _)| *c)
                    .collect()
            })
            .unwrap_or_default();
        for conn in conns {
            let Some(host) = self.lan.as_ref() else {
                continue;
            };
            let Some(g) = host
                .hub
                .guests
                .iter()
                .find(|g| g.conn == conn && host.hub.here(g))
            else {
                continue;
            };
            let Some(gs) = host.menus.get(&conn).and_then(|p| p.station.as_ref()) else {
                continue;
            };
            let (window, beacon, pos) = (gs.window, gs.kind == StationKind::Beacon, gs.pos);
            let invalid = !self.lan_station_valid(g, gs);
            if invalid {
                self.lan_station_closed(conn, Some(window));
                if let Some(host) = self.lan.as_mut() {
                    host.hub.send_to(
                        conn,
                        &C::ContainerClose(play::ContainerClose { container: window }),
                    );
                }
            } else if beacon {
                // The beacon's data slots follow its block entity.
                let data = self.beacon_data(pos);
                if let Some(gs) = self
                    .lan
                    .as_mut()
                    .and_then(|host| host.menus.get_mut(&conn))
                    .and_then(|p| p.station.as_mut())
                    && gs.kind == StationKind::Beacon
                    && gs.menu.data[..] != data[..]
                {
                    gs.menu.data.copy_from_slice(&data);
                    self.lan_send_station(conn);
                }
            }
        }
    }
}

impl Game<'_> {
    /// A station menu's `stillValid`: its block is still that station and the
    /// guest is within `isWithinBlockInteractionRange(pos, 4)`.
    pub(super) fn lan_station_valid(
        &self,
        g: &myriade_engine::lan::guest::Guest,
        gs: &GuestStation,
    ) -> bool {
        let [x, y, z] = gs.pos;
        let name = &self.res.facts().get(self.world.block(x, y, z)).name;
        StationKind::from_block(name) == Some(gs.kind) && g.within_block_range(gs.pos, 4.0)
    }

    /// `LecternMenu.stillValid` (`Container.stillValidBlockEntity`): the block
    /// entity still holds a book and the guest is within the interaction
    /// range and its buffer.
    pub(super) fn lan_lectern_valid(
        &self,
        g: &myriade_engine::lan::guest::Guest,
        key: (i32, i32, i32),
    ) -> bool {
        matches!(
            self.world.block_entities.get(&key),
            Some(BlockEntity::Lectern(l)) if l.book.is_some()
        ) && g.within_block_range([key.0, key.1, key.2], 4.0)
    }

    /// The guests' lecterns each tick: `stillValid` (the block entity, its
    /// book, the reach) closes them, and `broadcastChanges` sends a book or
    /// page another player changed.
    fn lan_lecterns_tick(&mut self) {
        let Some(host) = self.lan.as_ref() else {
            return;
        };
        let open: Vec<(u32, i32, (i32, i32, i32))> = host
            .play
            .iter()
            .filter_map(|(c, p)| p.lectern.as_ref().map(|l| (*c, l.window, l.key)))
            .collect();
        for (conn, window, key) in open {
            let Some(host) = self.lan.as_ref() else {
                return;
            };
            let Some(g) = host.hub.guests.iter().find(|g| g.conn == conn) else {
                continue;
            };
            if !host.hub.here(g) {
                continue;
            }
            if self.lan_lectern_valid(g, key) {
                self.lan_send_lectern(conn, false);
            } else {
                self.lan_station_closed(conn, Some(window));
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

/// What `clearContainer` gives back from a station's menu.
pub(super) fn station_inputs(gs: &GuestStation) -> Vec<ItemStack> {
    menu_of(gs.kind)
        .map(|(_, _, inputs)| inputs.filter_map(|s| gs.menu.slot(s).cloned()).collect())
        .unwrap_or_default()
}

