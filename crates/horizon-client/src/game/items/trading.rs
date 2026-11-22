//! The merchant (villager trading) menu: `MerchantMenu` / `MerchantScreen`
//! layout (276x166, drawn 50 px left of the usual 176-wide origin), offer
//! buttons, payment slots, the result slot's `onTake`, villager experience and
//! level-ups with new offers from the next trade set.
use super::{
    Game,
    stations::{Kind, Station},
};
use horizon_client::mobs::{Species, VILLAGER_TYPES, villager};
use horizon_client::{
    items::ItemStack,
    merchant::{self, MerchantOffer, TradeContext},
    random_sequences::RandomSequences,
    text::Text,
};

pub(super) struct MerchantState {
    /// The merchant's side of the menu: offers, experience, level, the
    /// selection hint and active offer (`horizon_client::merchant::Session`).
    pub(super) session: Box<merchant::Session>,
    /// The trading mob (its offers and experience are written back on
    /// close) and its custom name.
    pub(super) mob: Option<u32>,
    /// `Merchant.getDisplayName()`: the trading mob's custom name with its
    /// style, else nothing (the profession's name is the default).
    pub(super) name: Option<Text>,
}
/// The merchant screen is 100 px wider than the 176-px menus it shares an origin with.
const SHIFT: f32 = -50.0;

/// Player inventory slot positions in `MerchantMenu` (x from 108).
pub(super) fn inventory_slot(i: usize) -> (f32, f32) {
    match i {
        0..9 => (108.0 + SHIFT + i as f32 * 18.0, 142.0),
        _ => (
            108.0 + SHIFT + ((i - 9) % 9) as f32 * 18.0,
            84.0 + ((i - 9) / 9) as f32 * 18.0,
        ),
    }
}

/// `ExplorationMapFunction` in the game world: the nearest structure of the
/// tag through the generator's `/locate`, a new map centred on it with its
/// target decoration.
#[derive(Clone)]
pub(super) struct GameMaps<'a> {
    pub(super) terrain: Option<std::sync::Arc<horizon_client::vanillagen::VanillaTerrain>>,
    /// Structure tag -> its structures.
    pub(super) structure_tag: &'a dyn Fn(&str) -> Vec<String>,
    /// Maps made, for the caller to register: (map id, centre x, z, scale, target).
    pub(super) made: std::cell::RefCell<Vec<(i32, [i32; 3], u8, String)>>,
    pub(super) next_id: std::cell::Cell<i32>,
}
impl merchant::MapMaker for GameMaps<'_> {
    fn with_staged(&self, edit: &mut dyn FnMut(&dyn merchant::MapMaker) -> bool) -> bool {
        let staged = self.clone();
        if !edit(&staged) {
            return false;
        }
        self.next_id.set(staged.next_id.get());
        self.made.replace(staged.made.into_inner());
        true
    }
    fn exploration_map(
        &self,
        stack: ItemStack,
        map: &merchant::ExplorationMap,
        origin: [f64; 3],
    ) -> ItemStack {
        let Some(terrain) = self.terrain.as_ref() else {
            return stack;
        };
        let names = (self.structure_tag)(&map.destination);
        if names.is_empty() {
            return stack;
        }
        let from = [
            origin[0].floor() as i32,
            origin[1].floor() as i32,
            origin[2].floor() as i32,
        ];
        let Some((_, at)) =
            terrain.locate_for_map(&names, from, map.search_radius, map.skip_existing_chunks)
        else {
            return stack;
        };
        let id = self.next_id.get();
        self.next_id.set(id + 1);
        self.made
            .borrow_mut()
            .push((id, at, map.zoom, map.decoration.clone()));
        // The trade's own map item (woodland_mansion_map, ...) keeps its id.
        let mut out = stack;
        let d = out.data_mut();
        d.map_id = Some(id);
        d.other.push((
            "minecraft:map_decorations".into(),
            horizon_core::nbt::Tag::compound([(
                "+",
                horizon_core::nbt::Tag::compound([
                    (
                        "type",
                        horizon_core::nbt::Tag::String(map.decoration.as_str().into()),
                    ),
                    (
                        "x",
                        horizon_core::nbt::Tag::Double(f64::from(at[0]).to_bits()),
                    ),
                    (
                        "z",
                        horizon_core::nbt::Tag::Double(f64::from(at[2]).to_bits()),
                    ),
                    (
                        "rotation",
                        horizon_core::nbt::Tag::Float(180.0f32.to_bits()),
                    ),
                ]),
            )]),
        ));
        out
    }
}

/// The world's `data/minecraft/random_sequences.dat`.
fn random_sequences_file(world_root: &std::path::Path) -> std::path::PathBuf {
    world_root.join("data/minecraft/random_sequences.dat")
}
/// `RandomSequences` as opening the world reads it. A world with no file gets new data,
/// which `MinecraftServer`'s `savedDataStorage.computeIfAbsent(RandomSequences.TYPE)` stores
/// dirty (`SavedDataStorage.set`): its first save writes it.
pub(super) fn read_random_sequences(world_root: &std::path::Path) -> RandomSequences {
    horizon_client::save::read_dat(&random_sequences_file(world_root))
        .and_then(|t| t.get("data").cloned())
        .map(|d| RandomSequences::from_nbt(&d))
        .unwrap_or_else(RandomSequences::new_dirty)
}
/// `SavedDataStorage.saveAndJoin` for `RandomSequences`: written only while dirty, with
/// whatever it holds then, so the emptied map and the salt a `/random reset *` left reach
/// the file after a draw, and not without one.
pub(super) fn write_random_sequences(world_root: &std::path::Path, data: &mut RandomSequences) {
    let Some(data) = data.take_dirty_nbt() else {
        return;
    };
    let tag = horizon_core::nbt::Tag::compound([
        ("data", data),
        (
            "DataVersion",
            horizon_core::nbt::Tag::Int(horizon_world::DATA_VERSION),
        ),
    ]);
    if let Err(e) = horizon_client::save::write_dat(&random_sequences_file(world_root), tag) {
        log::warn!("the villager random sequences were not written: {e}");
    }
}

impl Game<'_> {
    /// `RandomSequences` as `random_sequences.dat` (not on a server's guest).
    pub(super) fn save_random_sequences(&mut self) {
        {
            write_random_sequences(&self.world_root, &mut self.rand_sequences);
        }
    }
    /// `AbstractVillager.addOffersFromTradeSet` with the world's random
    /// sequence of the set, at the merchant's position.
    fn offers_from_set(
        &mut self,
        set: &str,
        variant: &str,
        origin: [f64; 3],
    ) -> Vec<MerchantOffer> {
        let Some(sequence) = self.stations().trades.sequence(set) else {
            return Vec::new();
        };
        let mut random = self.rand_sequences.lend(&sequence, self.level.seed);
        let next_map = self.peek_map_id();
        let tag = |t: &str| self.structure_tag(t);
        let maps = GameMaps {
            terrain: self.end.locator(),
            structure_tag: &tag,
            made: Default::default(),
            next_id: std::cell::Cell::new(next_map),
        };
        let ctx = TradeContext {
            items: self.items(),
            enchantments: &self.stations().enchantments,
            variant,
            origin,
            maps: Some(&maps),
        };
        let offers = self
            .stations()
            .trades
            .offers_from_set(&ctx, set, &mut random);
        let made = maps.made.into_inner();
        self.rand_sequences.give_back(sequence, random);
        self.register_exploration_maps(made);
        offers
    }
    /// `AbstractVillager.getOffers`: generated on first use.
    pub(super) fn ensure_offers(&mut self, index: usize) {
        let species = self.remote_entities[index].species();
        if species == Species::Villager
            && self
                .stations()
                .professions
                .state_index(&self.remote_entities[index].extra.profession)
                .is_none()
        {
            return;
        }
        let unset = self.remote_entities[index]
            .extra
            .trade
            .as_ref()
            .is_none_or(|t| t.unset);
        if unset {
            let m = &self.remote_entities[index];
            let variant = format!(
                "minecraft:{}",
                VILLAGER_TYPES[m.extra.variant as usize % VILLAGER_TYPES.len()]
            );
            let origin = [m.position.x, m.position.y, m.position.z];
            let offers = if species == Species::WanderingTrader {
                let mut v = Vec::new();
                for set in ["buying", "uncommon", "common"] {
                    v.extend(self.offers_from_set(
                        &format!("minecraft:wandering_trader/{set}"),
                        &variant,
                        origin,
                    ));
                }
                v
            } else {
                let profession = m.extra.profession.id();
                let level = m.extra.villager_level.max(1);
                let set = self.stations().trades.set_for(&profession, level);
                if self.stations().trades.sets.contains_key(&set) {
                    self.offers_from_set(&set, &variant, origin)
                } else {
                    Vec::new()
                }
            };
            let trade = self.remote_entities[index].extra.trade.get_or_insert_default();
            trade.offers = offers;
            trade.unset = false;
        }
    }
    /// `Villager.mobInteract` / `WanderingTrader.mobInteract`: shake the
    /// head without offers, else open the trading screen.
    pub(super) fn interact_trader(
        &mut self,
        index: usize,
        item: &str,
    ) -> horizon_client::Result<bool> {
        let m = &self.remote_entities[index];
        let species = m.species();
        if !matches!(species, Species::Villager | Species::WanderingTrader)
            || !m.alive()
            || item == "minecraft:villager_spawn_egg"
            || self.station.is_some()
            || m.extra.flag2
            || (species == Species::Villager && villager::sleeping(m))
        {
            return Ok(false);
        }
        if m.is_baby() {
            // A baby villager shakes its head; a baby wandering trader does
            // nothing of its own.
            if species == Species::WanderingTrader {
                return Ok(false);
            }
            let mut events = Vec::new();
            villager::set_unhappy(&mut self.remote_entities[index], &mut events);
            self.handle_mob_events(events)?;
            return Ok(true);
        }
        let unresolved = species == Species::Villager
            && self
                .stations()
                .professions
                .state_index(&m.extra.profession)
                .is_none();
        self.award_custom_stat("talked_to_villager", 1);
        if unresolved {
            let mut events = Vec::new();
            villager::set_unhappy(&mut self.remote_entities[index], &mut events);
            self.handle_mob_events(events)?;
            return Ok(true);
        }
        self.ensure_offers(index);
        let m = &self.remote_entities[index];
        let trade = m.extra.trade.as_deref().cloned().unwrap_or_default();
        if trade.offers.is_empty() {
            if species == Species::Villager {
                let mut events = Vec::new();
                villager::set_unhappy(&mut self.remote_entities[index], &mut events);
                self.handle_mob_events(events)?;
            }
            return Ok(true);
        }
        let villager = species == Species::Villager;
        let profession = if villager {
            m.extra.profession.id()
        } else {
            "minecraft:wandering_trader".into()
        };
        let level = if villager {
            m.extra.villager_level.max(1)
        } else {
            1
        };
        let mut session = merchant::Session::new(trade.offers, villager);
        session.xp = trade.xp;
        session.level = level;
        session.profession = profession;
        session.variant = format!(
            "minecraft:{}",
            VILLAGER_TYPES[m.extra.variant as usize % VILLAGER_TYPES.len()]
        );
        session.random.clone_from(&m.random);
        session.origin = [m.position.x, m.position.y, m.position.z];
        session.ambient_sound_time = m.ambient_timer;
        let (id, name) = (
            m.id,
            m.custom_name.as_ref().map(|plain| {
                m.custom_name_text
                    .clone()
                    .unwrap_or_else(|| Text::literal(plain))
            }),
        );
        if villager {
            let player = self.acting_player_uuid_id();
            session.reputation = player.map_or(0, |p| self.ecs.villager_reputation_of(id, p));
            session.hero = player.and_then(|p| self.ecs.player_hero(p));
            for seq in merchant::Session::sequences_needed(
                &self.stations().trades,
                &session.profession,
                level,
            ) {
                let r = self.rand_sequences.lend(&seq, self.level.seed);
                session.sequences.insert(seq, r);
            }
        }
        // startTrading: updateSpecialPrices, then the screen opens.
        session.update_special_prices();
        // The mob owns the live offers even while a menu is open.
        {
            let trade = self.remote_entities[index].extra.trade.get_or_insert_default();
            trade.offers = session.offers.clone();
        }
        // setTradingPlayer: a wandering trader stands still while trading, a
        // villager's brain follows and looks at the player
        // (LookAndFollowTradingPlayerSink).
        self.remote_entities[index].extra.flag2 = true;
        self.open_merchant_state(MerchantState {
            session: Box::new(session),
            mob: Some(id),
            name,
        });
        Ok(true)
    }
    /// `ensure_offers` for the ECS trader `id`: P's `trade_offers::ensure_offers` decides and draws the offers.
    pub(super) fn ensure_offers_ecs(&mut self, id: u32) {
        self.ecs.ensure_offers(id);
    }
    /// `interact_trader` for the ECS trader `id`: the screen is P's `trade_session::open`, the session is built from the bridge's trade state.
    pub(in crate::game) fn interact_trader_ecs(&mut self, id: u32, item: &str) -> horizon_client::Result<bool> {
        let Some(villager) = self.ecs.is_villager(id) else { return Ok(false) };
        if !self.mob_alive(id)
            || item == "minecraft:villager_spawn_egg"
            || self.station.is_some()
            || self.ecs.trade_busy(id)
            || (villager && self.ecs.flag(id, horizon_ecs_proto::bridge::host::Flag::Sleeping) == Some(true))
        {
            return Ok(false);
        }
        if self.ecs_mob(id).is_some_and(|m| m.baby) {
            if !villager {
                return Ok(false);
            }
            self.ecs.set_unhappy(id);
            return Ok(true);
        }
        if villager && self.ecs.trader_opaque(id) {
            self.ecs.set_unhappy(id);
            return Ok(true);
        }
        self.award_custom_stat("talked_to_villager", 1);
        self.ensure_offers_ecs(id);
        if self.ecs.trade_offers(id).is_none_or(|offers| offers.is_empty()) {
            if villager {
                self.ecs.set_unhappy(id);
            }
            return Ok(true);
        }
        let Some(player) = self.acting_player_uuid_id() else { return Ok(false) };
        let name = self.ecs.custom_name(id).map(|plain| Text::literal(&plain));
        self.ecs.trade_open(id, player);
        let Some(session) = self.ecs.merchant_session(id) else { return Ok(false) };
        self.open_merchant_state(MerchantState {
            session: Box::new(session),
            mob: Some(id),
            name,
        });
        Ok(true)
    }
    /// Apply the villager AI's request to generate its offers.
    pub(super) fn merchant_offers_generated(&mut self, mob: u32) {
        if let Some(i) = self.remote_entities.iter().position(|m| m.id == mob) {
            self.ensure_offers(i);
        }
    }
    /// `Villager.updateSpecialPrices` after restock: the live offers, result
    /// slot and offer packet all change before the trading player's next click.
    pub(super) fn merchant_restocked(&mut self, id: u32) {
        self.merchant_refresh_local(id);
        self.lan_merchant_reputation(id);
    }
    /// `Villager.updateSpecialPrices` for the current local trading player.
    fn merchant_refresh_local(&mut self, id: u32) {
        if !self
            .station
            .as_ref()
            .and_then(|s| s.merchant.as_ref())
            .is_some_and(|m| m.mob == Some(id))
        {
            return;
        }
        let Some(player) = self.player_uuid_of(self.lan_acting).and_then(|uuid| self.player_id(uuid)) else {
            return;
        };
        let reputation = self.ecs.villager_reputation_of(id, player);
        let hero = self.ecs.player_hero(player);
        self.menu_session(|menu, env, _| {
            if let Some(session) = menu.merchant.as_mut() {
                session.reputation = reputation;
                session.hero = hero;
                session.update_special_prices();
            }
            menu.merchant_update(env);
            Some(true)
        });
    }
    /// `Villager.setTradingPlayer(null)`: retain the current special prices;
    /// the next opening recalculates them. Return borrowed sequences to the world.
    pub(super) fn merchant_closed(&mut self, state: &mut MerchantState) {
        self.merchant_sync_in(&mut state.session, state.mob);
        let session = &mut state.session;
        for (id, r) in std::mem::take(&mut session.sequences) {
            self.rand_sequences.give_back(id, r);
        }
        let Some(id) = state.mob else { return };
        let professions = self.stations().professions.clone();
        let Some(m) = self.remote_entities.iter_mut().find(|m| m.id == id) else {
            return;
        };
        m.extra.flag2 = false;
        if session.villager && professions.state_index(&m.extra.profession).is_none() {
            return;
        }
        if session.villager {
            m.extra.villager_level = session.level.max(1);
        }
        m.extra.flag2 = false;
        *m.random = session.random.clone();
        m.ambient_timer = session.ambient_sound_time;
        let trade = m.extra.trade.get_or_insert_default();
        trade.xp = session.xp;
        trade.offers = session.offers.clone();
    }
    fn open_merchant_state(&mut self, state: MerchantState) {
        let mut s = Station::new_kind(Kind::Merchant, [0, 0, 0]);
        s.merchant = Some(Box::new(state));
        self.station = Some(s);
        self.right = false;
    }
    /// The trading mob's random before a menu click (it ticks meanwhile).
    pub(super) fn merchant_sync_in(&self, session: &mut merchant::Session, mob: Option<u32>) {
        if let Some(m) = mob.and_then(|id| self.remote_entities.iter().find(|m| m.id == id)) {
            session.random.clone_from(&m.random);
            session.ambient_sound_time = m.ambient_timer;
            session.origin = [m.position.x, m.position.y, m.position.z];
            if let Some(trade) = m.extra.trade.as_deref() {
                session.offers = trade.offers.clone();
                session.xp = trade.xp;
            }
        }
    }
    /// After a menu click: the session back to the station, its random to
    /// the mob, and what it did (orbs, level-ups, the yes/no voice).
    pub(super) fn merchant_sync_out(&mut self, session: Box<merchant::Session>) {
        let mob = self
            .station
            .as_ref()
            .and_then(|s| s.merchant.as_ref())
            .and_then(|m| m.mob);
        if mob
            .and_then(|id| self.remote_entities.iter().find(|m| m.id == id))
            .is_some_and(|m| {
                m.species() == Species::Villager
                    && self
                        .stations()
                        .professions
                        .state_index(&m.extra.profession)
                        .is_none()
            })
        {
            if let Some(m) = self.station.as_mut().and_then(|s| s.merchant.as_mut()) {
                m.session = session;
            }
            return;
        }
        let (mut session, traded) = self.merchant_apply(session, mob);
        self.merchant_traded(mob, &mut session.offers, &traded);
        if let Some(m) = self.station.as_mut().and_then(|s| s.merchant.as_mut()) {
            m.session = session;
        }
    }
    /// What a menu click's session did, applied to the world and its mob;
    /// with the index of the offer of each trade made, for `merchant_traded`.
    pub(super) fn merchant_apply(
        &mut self,
        session: Box<merchant::Session>,
        mob: Option<u32>,
    ) -> (Box<merchant::Session>, Vec<usize>) {
        let mut session = session;
        let events = std::mem::take(&mut session.events);
        let mut traded = Vec::new();
        let (villager, origin) = (session.villager, session.origin);
        if let Some(m) = mob.and_then(|id| self.remote_entities.iter_mut().find(|m| m.id == id)) {
            *m.random = session.random.clone();
            m.ambient_timer = session.ambient_sound_time;
            if villager {
                m.extra.villager_level = session.level.max(1);
            }
        }
        for e in events {
            match e {
                merchant::Event::OffersRefreshed => {}
                merchant::Event::Orb(v) => {
                    self.add_orb([origin[0], origin[1] + 0.5, origin[2]], v as u32)
                }
                merchant::Event::LevelUp(_) => {
                    if let Some(m) = mob.and_then(|id| self.remote_entities.iter_mut().find(|m| m.id == id)) {
                        m.add_effect("minecraft:regeneration", 200, 0);
                    }
                }
                merchant::Event::TradeUpdated(yes, pitch) => {
                    let sound = match (villager, yes) {
                        (true, true) => "minecraft:entity.villager.yes",
                        (true, false) => "minecraft:entity.villager.no",
                        (false, true) => "minecraft:entity.wandering_trader.yes",
                        (false, false) => "minecraft:entity.wandering_trader.no",
                    };
                    self.audio.play(sound, Some(origin), 1.0, pitch);
                }
                merchant::Event::Traded(index, _offer) => {
                    traded.push(index);
                }
                merchant::Event::TradeSound => {
                    let sound = if villager {
                        "minecraft:entity.villager.yes"
                    } else {
                        "minecraft:entity.wandering_trader.yes"
                    };
                    self.audio.play(sound, Some(origin), 1.0, 1.0);
                }
            }
        }
        // Offer hooks above may edit the live session. Publish that final state
        // so the next click, AI restock and save all see the same offers.
        if let Some(m) = mob.and_then(|id| self.remote_entities.iter_mut().find(|m| m.id == id)) {
            let trade = m.extra.trade.get_or_insert_default();
            trade.offers = session.offers.clone();
            trade.xp = session.xp;
        }
        (session, traded)
    }
    /// `MerchantResultSlot.onTake` for each trade made (`traded` indices into
    /// the session's live `offers`): the statistic, `AbstractVillager.notifyTrade`'s
    /// `villager_trade` trigger, then the mods' `native_trade_with_villager`
    /// for a server's player (when the trader is still in the level), which
    /// may change the live offer (Bukkit's `PlayerTradeEvent#getTrade`).
    pub(super) fn merchant_traded(
        &mut self,
        mob: Option<u32>,
        offers: &mut [MerchantOffer],
        traded: &[usize],
    ) {
        for index in traded {
            let Some(offer) = offers.get_mut(*index) else {
                continue;
            };
            self.award_custom_stat("traded_with_villager", 1);
            let merchant = mob.and_then(|id| self.remote_entities.iter().find(|m| m.id == id));
            let kind = merchant.map(super::native_hooks::mob_kind);
            let villager = merchant.map(|m| self.mob_entity(m));
            if let Some(villager) = villager {
                self.trigger_trade(&villager, &offer.result);
            }
            // TradeWithVillagerEvent: on the server, in the player's turn.
            if let (Some(id), Some(kind), Some(_)) = (mob, kind, self.lan_acting) {
                self.native_trade_with_villager(id, &kind, offer);
            }
        }
    }
}

#[cfg(test)]
#[path = "tests/random_sequences_save_tests.rs"]
mod random_sequences_save_tests;
