//! Trading for LAN guests: the villager screen a guest opens runs the same
//! merchant menu as the host's (`myriade_engine::menu`, `Kind::Merchant`),
//! with vanilla's packets: OpenScreen (`minecraft:merchant`), MerchantOffers,
//! the select trade packet, container clicks and the content sync after each.
use super::{
    Game, lan_actions::guest_within_entity_range, stations::Kind as StationKind,
    trading::MerchantState,
};
use myriade_engine::{
    items::ItemStack,
    menu::{Env, Input, Kind, Menu},
    merchant,
};
use horizon_protocol::packets::play::{self, Clientbound as C};

/// A guest's open trading screen.
pub(super) struct GuestMerchant {
    pub(super) window: i32,
    state: i32,
    /// The payment slots and the result.
    slots: [Option<ItemStack>; 3],
    pub(super) merchant: Box<MerchantState>,
    /// The offers the client was last sent.
    sent: Vec<merchant::MerchantOffer>,
    sent_level: i32,
    sent_xp: i32,
}

/// A guest's UUID as gossip targets it.
fn uuid_words(uuid: u128) -> [i32; 4] {
    [
        (uuid >> 96) as i32,
        (uuid >> 64) as i32,
        (uuid >> 32) as i32,
        uuid as i32,
    ]
}

impl Game<'_> {
    /// Development oracle: observe the actual open guest merchant, never a synthetic menu.
    pub(in crate::game) fn harness_merchant(
        &self,
        host: &super::lan::LanHost,
        conn: u32,
    ) -> Option<(i32, i32, serde_json::Value)> {
        use serde_json::json;
        let gm = host.play.get(&conn)?.merchant.as_ref()?;
        let guest = host
            .hub
            .guests
            .iter()
            .find(|g| g.conn == conn && g.placed)?;
        let id = gm.merchant.mob?;
        self.mob_alive(id).then_some(())?;
        let stack = |s: &Option<ItemStack>| s.as_ref().map(|s| json!([s.item.as_ref(), s.count]));
        let session = &gm.merchant.session;
        Some((
            gm.window,
            gm.state,
            json!({"window":gm.window,"merchant":self.persist.uuids.get(&id)?,
            "player":uuid_words(guest.uuid),"slots":gm.slots.iter().map(stack).collect::<Vec<_>>(),
            "carried":stack(&guest.carried),"xp":session.xp,
            "sent_offers":gm.sent.iter().map(|o|json!({"uses":o.uses,
                "special":o.special_price_diff,"demand":o.demand,"cost":o.cost_a_count(self.items())})).collect::<Vec<_>>(),
            "offers":session.offers.iter().map(|o|json!({"uses":o.uses,
                "special":o.special_price_diff,"demand":o.demand,"cost":o.cost_a_count(self.items())})).collect::<Vec<_>>()}),
        ))
    }

    /// A trading screen the guest's interaction opened (it opened as a
    /// station while the guest acted): handed to the guest.
    pub(super) fn lan_open_merchant(&mut self, conn: u32, mut state: MerchantState) {
        let Some(uuid) = self
            .lan
            .as_ref()
            .and_then(|host| host.hub.guests.iter().find(|g| g.conn == conn))
            .map(|g| uuid_words(g.uuid))
        else {
            return;
        };
        if let Some(id) = state.mob
            && state.session.villager
        {
            // startTrading with the guest's reputation. The session already
            // captured their Hero effect while they acted as the player.
            state.session.reputation = self.player_id(uuid).map_or(0, |p| self.ecs.villager_reputation_of(id, p));
            state.session.update_special_prices();
        }
        if let Some(id) = state.mob {
            self.ecs.set_trade_offers(id, state.session.offers.clone());
        }
        let title = self.merchant_display_name(&state);
        if !self.guest_may_open(conn, "minecraft:merchant", None) {
            self.merchant_closed(&mut state);
            return;
        }
        // ServerPlayer.openMenu closes the menu that was open.
        self.guest_close_all(conn);
        let Some(window) = self
            .lan
            .as_mut()
            .and_then(|host| host.hub.open_merchant_window(conn, title))
        else {
            self.merchant_closed(&mut state);
            return;
        };
        let mut gm = GuestMerchant {
            window,
            state: 0,
            slots: [None, None, None],
            merchant: Box::new(state),
            sent: Vec::new(),
            sent_level: 0,
            sent_xp: 0,
        };
        self.lan_send_offers(conn, &mut gm, true);
        self.lan_send_merchant_content(conn, &mut gm);
        if let Some(host) = self.lan.as_mut() {
            host.play.entry(conn).or_default().merchant = Some(gm);
        }
    }
    /// `Entity.getDisplayName` of the merchant, the screen's title: its name
    /// (custom, else the profession's) with the show_entity hover event and
    /// its UUID as insertion.
    fn merchant_display_name(&mut self, state: &MerchantState) -> horizon_core::nbt::Tag {
        use horizon_core::nbt::Tag;
        let s = &state.session;
        let (kind, key) = if s.villager {
            let p = s.profession.trim_start_matches("minecraft:");
            (
                "minecraft:villager",
                format!("entity.minecraft.villager.{p}"),
            )
        } else {
            (
                "minecraft:wandering_trader",
                "entity.minecraft.wandering_trader".to_owned(),
            )
        };
        let name = match &state.name {
            Some(n) => myriade_engine::scoreboard::json_to_tag(&n.component()),
            None => Tag::compound([("translate", Tag::String(key.as_str().into()))]),
        };
        let uuid = state.mob.map_or([0; 4], |id| self.mob_uuid(id));
        let w = uuid.map(|v| v as u32);
        let text = format!(
            "{:08x}-{:04x}-{:04x}-{:04x}-{:04x}{:08x}",
            w[0],
            w[1] >> 16,
            w[1] & 0xffff,
            w[2] >> 16,
            w[2] & 0xffff,
            w[3]
        );
        let mut out = match name.clone() {
            Tag::Compound(c) => c,
            other => {
                let mut c = horizon_core::nbt::Compound::new();
                c.insert("text".into(), other);
                c
            }
        };
        out.insert(
            "hover_event".into(),
            Tag::compound([
                ("action", Tag::String("show_entity".into())),
                ("id", Tag::String(kind.into())),
                ("uuid", Tag::IntArray(uuid.to_vec())),
                ("name", name),
            ]),
        );
        out.insert("insertion".into(), Tag::String(text.as_str().into()));
        Tag::Compound(out)
    }
    /// The screen title: the custom name, else the profession (the client
    /// adds the level itself).
    /// `ServerPlayer.sendMerchantOffers`, when the offers changed.
    fn lan_send_offers(&mut self, conn: u32, gm: &mut GuestMerchant, force: bool) {
        let s = &gm.merchant.session;
        if !force && s.offers == gm.sent && s.level == gm.sent_level && s.xp == gm.sent_xp {
            return;
        }
        if let Some(host) = self.lan.as_mut() {
            let offers: Vec<play::NetOffer> = s
                .offers
                .iter()
                .filter_map(|o| {
                    let cost = |st: &ItemStack, hub: &mut myriade_engine::lan::hub::Hub| {
                        hub.stack(&Some(st.clone())).map(play::NetCost::from)
                    };
                    Some(play::NetOffer {
                        cost_a: cost(&o.base_cost_a.stack, &mut host.hub)?,
                        result: host.hub.stack(&Some(o.result.clone()))?,
                        cost_b: match &o.cost_b {
                            Some(b) => Some(cost(&b.stack, &mut host.hub)?),
                            None => None,
                        },
                        out_of_stock: o.out_of_stock(),
                        uses: o.uses,
                        max_uses: o.max_uses,
                        xp: o.xp,
                        special_price: o.special_price_diff,
                        price_multiplier: o.price_multiplier,
                        demand: o.demand,
                    })
                })
                .collect();
            host.hub.send_to(
                conn,
                &C::MerchantOffers(play::MerchantOffers {
                    container: gm.window,
                    offers,
                    level: s.level,
                    xp: s.xp,
                    show_progress: s.villager,
                    can_restock: s.villager,
                }),
            );
            gm.sent = s.offers.clone();
            gm.sent_level = s.level;
            gm.sent_xp = s.xp;
        }
    }
    fn lan_send_merchant_content(&mut self, conn: u32, gm: &mut GuestMerchant) {
        if let Some(host) = self.lan.as_mut() {
            let Some(gi) = host.hub.guests.iter().position(|g| g.conn == conn) else {
                return;
            };
            let mut items: Vec<Option<ItemStack>> = gm.slots.to_vec();
            let g = &host.hub.guests[gi];
            items.extend((9..36).chain(0..9).map(|i| g.inventory[i].clone()));
            let carried = g.carried.clone();
            let net: Vec<_> = items.iter().map(|s| host.hub.stack(s)).collect();
            let carried = host.hub.stack(&carried);
            gm.state = (gm.state + 1) & 0x7FFF;
            host.hub.send_to(
                conn,
                &C::ContainerSetContent(play::ContainerSetContent {
                    container: gm.window,
                    state: gm.state,
                    items: net,
                    carried,
                }),
            );
        }
    }
    /// Runs `op` on the guest's merchant menu (their inventory copied in and
    /// back), then syncs the client. False when the guest has no such window.
    fn lan_merchant_op(
        &mut self,
        conn: u32,
        window: Option<i32>,
        refresh_offers: bool,
        op: impl FnOnce(&mut Menu, &Env),
    ) -> bool {
        let Some(mut gm) = self
            .lan
            .as_mut()
            .and_then(|host| host.play.get_mut(&conn))
            .and_then(|p| p.merchant.take())
        else {
            return false;
        };
        if window.is_some_and(|w| w != gm.window) {
            if let Some(host) = self.lan.as_mut() {
                host.play.entry(conn).or_default().merchant = Some(gm);
            }
            return false;
        }
        let Some(gi) = self
            .lan
            .as_ref()
            .and_then(|host| host.hub.guests.iter().position(|g| g.conn == conn))
        else {
            return false;
        };
        let mut session = gm.merchant.session.clone();
        self.merchant_sync_in(&mut session, gm.merchant.mob);
        let next_map = self.peek_map_id();
        let tag = |t: &str| self.structure_tag(t);
        let maps = super::trading::GameMaps {
            terrain: self.end.locator(),
            structure_tag: &tag,
            made: Default::default(),
            next_id: std::cell::Cell::new(next_map),
        };
        let trade_env = merchant::TradeEnv {
            trades: &self.stations().trades,
            enchantments: &self.stations().enchantments,
            maps: Some(&maps),
        };
        let Some(host) = self.lan.as_ref() else {
            return false;
        };
        let g = &host.hub.guests[gi];
        let (player_uuid, player_entity_id) = (g.uuid, g.entity_id);
        let env = Env {
            crafting_remainder: Some(self),
            player: Some(player_uuid),
            player_entity_id: Some(player_entity_id),
            loop_context: &self.loop_context,
            items: self.items(),
            recipes: self.recipes(),
            brewing: &self.stations().brewing,
            banners: &self.stations().banners,
            station_recipes: &self.stations().recipes,
            enchantments: &self.stations().enchantments,
            names: &self.pack.names,
            trade: Some(&trade_env),
            maps: None,
            limited_crafting: host.hub.limited_crafting,
            can_equip: None,
            known_recipes: Some(&g.recipe_book),
            slot_rules: None,
        };
        let mut menu = Menu::new(Kind::Merchant);
        menu.set_registry_id(Some("minecraft:merchant"));
        if let Some(host) = self.lan.as_ref() {
            let g = &host.hub.guests[gi];
            for i in 0..36 {
                menu.set_player_item(i, g.inventory[i].clone());
            }
            menu.set_player_item(40, g.offhand.clone());
            menu.selected = g.selected;
            menu.creative = g.creative();
            menu.carried = g.carried.clone();
        }
        for (i, s) in gm.slots.iter().enumerate() {
            menu.set_slot(&env, i, s.clone());
        }
        menu.merchant = Some(session);
        op(&mut menu, &env);
        let made = maps.made.take();
        self.guest_bundle_sounds(gi, std::mem::take(&mut menu.player_sounds));
        let session = menu.merchant.take().expect("attached");
        let dropped = std::mem::take(&mut menu.dropped);
        let crafted_counts = std::mem::take(&mut menu.crafted_counts);
        gm.slots = [
            menu.slot(0).cloned(),
            menu.slot(1).cloned(),
            menu.slot(2).cloned(),
        ];
        if let Some(host) = self.lan.as_mut() {
            host.hub.guests.update(gi, |g| {
                for i in 0..36 {
                    g.inventory[i] = menu.player_item(i).cloned();
                }
                g.offhand = menu.player_item(40).cloned();
                g.carried = menu.carried.take();
            });
        }
        self.register_exploration_maps(made);
        if !crafted_counts.is_empty() {
            self.as_guest(conn, |g| {
                for (item, count) in crafted_counts {
                    g.award_stat("crafted", &item, count);
                }
            });
        }
        let opaque = gm.merchant.mob.is_some_and(|id| self.trader_opaque(id));
        let offers_refreshed = session
            .events
            .iter()
            .any(|event| matches!(event, merchant::Event::OffersRefreshed));
        let (mut session, traded) = if opaque {
            (session, Vec::new())
        } else {
            self.merchant_apply(session, gm.merchant.mob)
        };
        if !traded.is_empty() {
            let mob = gm.merchant.mob;
            self.as_guest(conn, |g| {
                g.merchant_traded(mob, &mut session.offers, &traded)
            });
        }
        gm.merchant.session = session;
        self.drop_guest_items(gi, dropped);
        if refresh_offers || offers_refreshed {
            self.lan_send_offers(conn, &mut gm, true);
        }
        self.lan_send_merchant_content(conn, &mut gm);
        if let Some(host) = self.lan.as_mut() {
            host.play.entry(conn).or_default().merchant = Some(gm);
        }
        true
    }
    /// Close an open merchant whose profession became unresolved, returning inputs.
    pub(super) fn close_opaque_merchant(&mut self, id: u32) {
        if self
            .station
            .as_ref()
            .and_then(|s| s.merchant.as_ref())
            .is_some_and(|m| m.mob == Some(id))
        {
            self.close_container();
        }
        if let Some(conn) = self.lan_trade_conn(id) {
            let window = self
                .lan
                .as_ref()
                .and_then(|h| h.play.get(&conn))
                .and_then(|p| p.merchant.as_ref())
                .map(|m| m.window);
            if let Some(window) = window
                && self.lan_merchant_closed(conn, Some(window))
                && let Some(host) = self.lan.as_mut()
            {
                host.hub.send_to(
                    conn,
                    &C::ContainerClose(play::ContainerClose { container: window }),
                );
            }
        }
    }

    /// A container click on the guest's trading window.
    pub(super) fn lan_merchant_click(&mut self, conn: u32, p: &play::ContainerClick) -> bool {
        let input = match p.input {
            0 => Input::Pickup,
            1 => Input::QuickMove,
            2 => Input::Swap,
            3 => Input::Clone,
            4 => Input::Throw,
            5 => Input::QuickCraft,
            _ => Input::PickupAll,
        };
        let (slot, button) = (i32::from(p.slot), i32::from(p.button));
        self.lan_merchant_op(conn, Some(p.container), false, |menu, env| {
            let _ = menu.click(env, slot, button, input);
        })
    }
    /// ServerboundSelectTradePacket: the selection hint, then tryMoveItems.
    pub(super) fn lan_select_trade(&mut self, conn: u32, index: i32) {
        let Ok(index) = usize::try_from(index) else {
            return;
        };
        self.lan_merchant_op(conn, None, false, |menu, env| {
            menu.merchant_select(env, index)
        });
    }
    /// The guest closed the trading window (or it is closed for them):
    /// MerchantMenu.removed, then the villager gets its state back.
    pub(super) fn lan_merchant_closed(&mut self, conn: u32, window: Option<i32>) -> bool {
        if !self.lan_merchant_op(conn, window, false, |menu, env| menu.merchant_close(env)) {
            return false;
        }
        let Some(mut gm) = self
            .lan
            .as_mut()
            .and_then(|host| host.play.get_mut(&conn))
            .and_then(|p| p.merchant.take())
        else {
            return false;
        };
        self.merchant_closed(&mut gm.merchant);
        if window.is_some() {
        }
        true
    }
    /// `Merchant.stillValid` for a trader: it is alive and the guest is within
    /// the entity interaction range plus 4 of its box.
    pub(super) fn lan_merchant_valid(
        &self,
        g: &myriade_engine::lan::guest::Guest,
        id: u32,
    ) -> bool {
        self.mob_alive(id)
            && !self.trader_opaque(id)
            && self.mob_box(id).is_some_and(|b| guest_within_entity_range(g, &b, 4.0))
    }
    /// `AbstractVillager.stillValid`: close invalid guest merchant screens.
    pub(super) fn lan_merchant_tick(&mut self) {
        // `AbstractVillager.stillValid`: the trade screen closes once the trader
        // is gone or dead, or the guest is more than 4 blocks past its reach.
        let far: Vec<(u32, i32)> = self
            .lan
            .as_ref()
            .map(|host| {
                host.play
                    .iter()
                    .filter_map(|(conn, p)| {
                        let gm = p.merchant.as_ref()?;
                        let id = gm.merchant.mob?;
                        let g = host
                            .hub
                            .guests
                            .iter()
                            .find(|g| g.conn == *conn && host.hub.here(g))?;
                        (!self.lan_merchant_valid(g, id)).then_some((*conn, gm.window))
                    })
                    .collect()
            })
            .unwrap_or_default();
        for (conn, window) in far {
            if self.lan_merchant_closed(conn, Some(window))
                && let Some(host) = self.lan.as_mut()
            {
                host.hub.send_to(
                    conn,
                    &C::ContainerClose(play::ContainerClose { container: window }),
                );
            }
        }
    }
    /// Refresh open guest prices after reputation changes or a restock.
    pub(super) fn lan_merchant_reputation(&mut self, id: u32) {
        let conns: Vec<u32> = self
            .lan
            .as_ref()
            .map(|host| {
                host.play
                    .iter()
                    .filter(|(_, p)| {
                        p.merchant.as_ref().is_some_and(|m| {
                            m.merchant.session.villager && m.merchant.mob == Some(id)
                        })
                    })
                    .map(|(c, _)| *c)
                    .collect()
            })
            .unwrap_or_default();
        for conn in conns {
            let Some((uuid, hero)) = self
                .lan
                .as_ref()
                .and_then(|host| {
                    host.hub
                        .guests
                        .iter()
                        .find(|g| g.conn == conn && host.hub.here(g))
                })
                .map(|g| (uuid_words(g.uuid), g.effects.amplifier("hero_of_the_village")))
            else {
                continue;
            };
            let reputation = self.player_id(uuid).map_or(0, |p| self.ecs.villager_reputation_of(id, p));
            self.lan_merchant_op(conn, None, true, |menu, env| {
                if let Some(s) = menu.merchant.as_mut() {
                    s.reputation = reputation;
                    s.hero = hero;
                    s.update_special_prices();
                }
                menu.merchant_update(env);
            });
        }
    }
    /// Takes a station a guest's action opened: a trading screen or a
    /// workstation menu goes to the guest (game/network/lan_stations.rs).
    pub(super) fn lan_station_opened(&mut self, conn: u32, station: super::stations::Station) {
        let mut station = station;
        if station.kind == StationKind::Merchant
            && let Some(m) = station.merchant.take()
        {
            self.lan_open_merchant(conn, *m);
            return;
        }
        self.lan_open_station(conn, &station);
    }
}

impl Game<'_> {
    /// The guest whose trading screen is open on mob `mob`.
    pub(super) fn lan_trade_conn(&self, mob: u32) -> Option<u32> {
        let host = self.lan.as_ref()?;
        host.play
            .iter()
            .find(|(_, p)| {
                p.merchant
                    .as_ref()
                    .is_some_and(|m| m.merchant.mob == Some(mob))
            })
            .map(|(conn, _)| *conn)
    }
    /// The live session of a guest's open trading screen, whose offers,
    /// experience and level are newer than the mob's until the screen closes.
    pub(super) fn lan_trade_session(&self, conn: u32) -> Option<&merchant::Session> {
        let gm = self.lan.as_ref()?.play.get(&conn)?.merchant.as_ref()?;
        Some(&gm.merchant.session)
    }
    /// The guest's player UUID.
    pub(super) fn lan_trade_player(&self, conn: u32) -> Option<u128> {
        Some(self.lan.as_ref()?.hub.guest(conn)?.uuid)
    }
    /// Changes the session of a guest's open trading screen and sends the
    /// client its offers and result slot again. The merchant's own random is
    /// not drawn: the update sound is held back.
    pub(super) fn lan_trade_edit(
        &mut self,
        conn: u32,
        edit: impl FnOnce(&mut merchant::Session),
    ) -> bool {
        let done = self.lan_merchant_op(conn, None, false, |menu, env| {
            let Some(session) = menu.merchant.as_mut() else {
                return;
            };
            edit(session);
            session.selection_hint = session
                .selection_hint
                .min(session.offers.len().saturating_sub(1));
            let sound_time = std::mem::replace(&mut session.ambient_sound_time, -80);
            menu.merchant_update(env);
            if let Some(session) = menu.merchant.as_mut() {
                session.ambient_sound_time = sound_time;
            }
        });
        if done {
            self.lan_trade_sequences(conn);
        }
        done
    }
    /// Lends the open session the world sequences of the levels above its
    /// level, and gives back the ones it no longer needs, so a level-up after
    /// the level was changed still rolls its trade set.
    fn lan_trade_sequences(&mut self, conn: u32) {
        let Some(session) = self.lan_trade_session(conn) else {
            return;
        };
        if !session.villager {
            return;
        }
        let needed = merchant::Session::sequences_needed(
            &self.stations().trades,
            &session.profession,
            session.level,
        );
        let Some(gm) = self
            .lan
            .as_mut()
            .and_then(|host| host.play.get_mut(&conn))
            .and_then(|p| p.merchant.as_mut())
        else {
            return;
        };
        let session = &mut gm.merchant.session;
        let stale: Vec<String> = session
            .sequences
            .keys()
            .filter(|id| !needed.contains(id))
            .cloned()
            .collect();
        for id in stale {
            if let Some(random) = session.sequences.remove(&id) {
                self.rand_sequences.give_back(id, random);
            }
        }
        for id in needed {
            if let std::collections::btree_map::Entry::Vacant(slot) = session.sequences.entry(id) {
                let random = self.rand_sequences.lend(slot.key(), self.level.seed);
                slot.insert(random);
            }
        }
    }
}
