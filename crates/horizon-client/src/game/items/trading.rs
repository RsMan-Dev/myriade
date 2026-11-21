//! The merchant (villager trading) menu: `MerchantMenu` / `MerchantScreen`
//! layout (276x166, drawn 50 px left of the usual 176-wide origin), offer
//! buttons, payment slots, the result slot's `onTake`, villager experience and
//! level-ups with new offers from the next trade set.
use super::{
    Game,
    stations::{INPUT, Kind, RESULT, Station},
};
use horizon_client::mobs::{Species, VILLAGER_TYPES, villager};
use horizon_client::{
    items::ItemStack,
    merchant::{self, MerchantOffer, TradeContext},
    random_sequences::RandomSequences,
    text::Text,
};

/// The merchant screen is 100 px wider than the 176-px menus it shares an origin with.
const SHIFT: f32 = -50.0;
const GREY: u32 = 0xFF40_4040;

pub(super) struct MerchantState {
    /// The merchant's side of the menu: offers, experience, level, the
    /// selection hint and active offer (`horizon_client::merchant::Session`).
    pub(super) session: Box<merchant::Session>,
    /// The offer list's scroll offset.
    pub(super) scroll: usize,
    /// The scroller is held by the mouse (`isDragging`).
    pub(super) dragging: bool,
    /// The offer button that keeps focus after a click (drawn highlighted).
    pub(super) focused: Option<usize>,
    /// The trading mob (its offers and experience are written back on
    /// close) and its custom name.
    pub(super) mob: Option<u32>,
    /// `Merchant.getDisplayName()`: the trading mob's custom name with its
    /// style, else nothing (the profession's name is the default).
    pub(super) name: Option<Text>,
}
/// `Merchant.getDisplayName`: the merchant's custom name (style and all), else
/// the profession's or the wandering trader's.
pub(super) fn merchant_display_name(
    progress: bool,
    profession: &str,
    custom: Option<Text>,
) -> Text {
    custom.unwrap_or_else(|| {
        if progress {
            Text::translate_with_fallback(
                &format!("entity.minecraft.villager.{profession}"),
                profession,
                &[],
            )
        } else {
            Text::translate_with_fallback(
                "entity.minecraft.wandering_trader",
                "Wandering Trader",
                &[],
            )
        }
    })
}
/// `MerchantScreen.extractLabels`: a villager's title is `merchant.title` with
/// the name and its level, so the name keeps the style it was given.
pub(super) fn merchant_title(progress: bool, level: i32, name: Text) -> Text {
    // `extractLabels` joins the level for levels 1 to 5 only (`VillagerData`
    // clamps a villager to 1; a level past 5 draws the name alone).
    if !progress || !(1..=5).contains(&level) {
        return name;
    }
    let level = Text::translate_with_fallback(&format!("merchant.level.{level}"), "", &[]);
    Text::translate_with_fallback("merchant.title", "%s - %s", &[name, level])
}
impl MerchantState {
    /// `Merchant.showProgressBar`: villagers only.
    pub(super) fn progress(&self) -> bool {
        self.session.show_progress()
    }
}
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
            scroll: 0,
            dragging: false,
            focused: None,
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
            scroll: 0,
            dragging: false,
            focused: None,
            mob: Some(id),
            name,
        });
        Ok(true)
    }
    /// `merchant_offers_generated` for the ECS trader `id`.
    pub(super) fn merchant_offers_generated_ecs(&mut self, id: u32) {
        self.ensure_offers_ecs(id);
    }
    /// `merchant_closed` for a session whose mob is the ECS trader `state.mob`: the mob keeps the special prices and the level.
    pub(super) fn merchant_closed_ecs(&mut self, state: &mut MerchantState) {
        self.merchant_sync_in_ecs(&mut state.session, state.mob);
        for (id, r) in std::mem::take(&mut state.session.sequences) {
            self.rand_sequences.give_back(id, r);
        }
        let Some(id) = state.mob else { return };
        self.ecs.trade_close(id);
        if state.session.villager && !self.ecs.trader_opaque(id) {
            self.ecs.set_trade_level(id, state.session.level.max(1));
        }
    }
    /// `merchant_sync_in` for an ECS trader: the offers, the xp, the random and the origin come from the bridge.
    pub(super) fn merchant_sync_in_ecs(&self, session: &mut merchant::Session, mob: Option<u32>) {
        let Some(id) = mob.filter(|id| self.ecs.is_active() && self.ecs.kind(*id).is_some()) else { return };
        if let Some(offers) = self.ecs.trade_offers(id) {
            session.offers = offers;
        }
        if let Some(random) = self.ecs.trade_random(id) {
            session.random = random;
        }
        if let Some(time) = self.ecs.ambient_timer(id) {
            session.ambient_sound_time = time;
        }
        session.origin = self.ecs.position(id).unwrap_or(session.origin);
    }
    /// `merchant_sync_out` for an ECS trader: the session goes back to the station, its random and its level to the bridge.
    pub(super) fn merchant_sync_out_ecs(&mut self, session: Box<merchant::Session>) {
        let mob = self.station.as_ref().and_then(|s| s.merchant.as_ref()).and_then(|m| m.mob);
        let villager_unknown = mob.is_some_and(|id| self.ecs.is_villager(id) == Some(true) && self.ecs.trader_opaque(id));
        if villager_unknown {
            if let Some(m) = self.station.as_mut().and_then(|s| s.merchant.as_mut()) {
                m.session = session;
            }
            return;
        }
        let (mut session, traded) = self.merchant_apply_ecs(session, mob);
        self.merchant_traded_ecs(mob, &mut session.offers, &traded);
        if let Some(m) = self.station.as_mut().and_then(|s| s.merchant.as_mut()) {
            m.session = session;
        }
    }
    /// `merchant_apply` for an ECS trader: the random, the level and the final offers are written to the bridge.
    pub(super) fn merchant_apply_ecs(&mut self, session: Box<merchant::Session>, mob: Option<u32>) -> (Box<merchant::Session>, Vec<usize>) {
        let mut session = session;
        let events = std::mem::take(&mut session.events);
        let mut traded = Vec::new();
        let (villager, origin) = (session.villager, session.origin);
        if let Some(id) = mob {
            self.ecs.set_trade_random(id, session.random.clone());
            self.ecs.set_ambient_timer(id, session.ambient_sound_time);
            if villager {
                self.ecs.set_trade_level(id, session.level.max(1));
            }
        }
        for e in events {
            match e {
                merchant::Event::OffersRefreshed => {}
                merchant::Event::Orb(v) => self.add_orb([origin[0], origin[1] + 0.5, origin[2]], v as u32),
                merchant::Event::LevelUp(_) => {
                    if let Some(e) = mob.and_then(|id| self.ecs.entity_of(id)) {
                        self.ecs.add_effect_named(e, "regeneration", 200, 0);
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
                merchant::Event::Traded(index, _offer) => traded.push(index),
                merchant::Event::TradeSound => {
                    let sound = if villager { "minecraft:entity.villager.yes" } else { "minecraft:entity.wandering_trader.yes" };
                    self.audio.play(sound, Some(origin), 1.0, 1.0);
                }
            }
        }
        if let Some(id) = mob {
            self.ecs.trade_store(id, session.offers.clone(), session.xp);
        }
        (session, traded)
    }
    /// `merchant_traded` for an ECS trader: the same statistic and triggers, the kind and the trade hook read by id.
    pub(super) fn merchant_traded_ecs(&mut self, mob: Option<u32>, offers: &mut [MerchantOffer], traded: &[usize]) {
        for index in traded {
            let Some(offer) = offers.get_mut(*index) else { continue };
            self.award_custom_stat("traded_with_villager", 1);
            let Some(id) = mob else { continue };
            let kind = self.ecs.kind(id).map(str::to_owned);
            self.trigger_trade(&self.ecs_predicate_view(id), &offer.result);
            if let (Some(kind), Some(_)) = (kind, self.lan_acting) {
                self.native_trade_with_villager(id, &kind, offer);
            }
        }
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
    /// Opens a merchant menu without a mob (horizon-ctl `trade`).
    pub(super) fn open_merchant(&mut self, profession: &str, variant: &str, level: i32) {
        let profession = if profession.contains(':') {
            profession.to_owned()
        } else {
            format!("minecraft:{profession}")
        };
        let variant = format!("minecraft:{}", variant.trim_start_matches("minecraft:"));
        let level = level.clamp(1, 5);
        let origin = self
            .player
            .as_ref()
            .map_or([0.0; 3], |p| [p.position.x, p.position.y, p.position.z]);
        let mut offers = Vec::new();
        for l in 1..=level {
            let set = self.stations().trades.set_for(&profession, l);
            offers.extend(self.offers_from_set(&set, &variant, origin));
        }
        let mut session = merchant::Session::new(offers, true);
        session.profession = profession;
        session.variant = variant;
        session.level = level;
        session.xp = merchant::min_xp(level);
        session.origin = origin;
        self.open_merchant_state(MerchantState {
            session: Box::new(session),
            scroll: 0,
            dragging: false,
            focused: None,
            mob: None,
            name: None,
        });
    }
    /// A server's trading window (no mob here: the server keeps it).
    pub(super) fn open_remote_merchant(&mut self, session: merchant::Session, name: Option<Text>) {
        self.open_merchant_state(MerchantState {
            session: Box::new(session),
            scroll: 0,
            dragging: false,
            focused: None,
            mob: None,
            name,
        });
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
    /// `MerchantMenu.tryMoveItems` for the selected offer (the select trade packet).
    fn try_move_items(&mut self, index: usize) {
        self.menu_session(|menu, env, _| {
            menu.merchant_select(env, index);
            Some(true)
        });
    }
    /// Clicks on the offer list; true when one was hit. A press on the
    /// scroller holds it (`MerchantScreen.mouseClicked`).
    pub(super) fn merchant_click(&mut self, local: (f32, f32)) -> bool {
        let Some(m) = self.station.as_mut().and_then(|s| s.merchant.as_mut()) else {
            return false;
        };
        m.dragging = on_scroller(local, m.session.offers.len());
        if m.dragging {
            return true;
        }
        for i in 0..7 {
            let (x, y) = (5.0 + SHIFT, 18.0 + 20.0 * i as f32);
            if local.0 >= x
                && local.0 < x + 88.0
                && local.1 >= y
                && local.1 < y + 20.0
                && i + m.scroll < m.session.offers.len()
            {
                let index = i + m.scroll;
                if let Some(m) = self.station.as_mut().and_then(|s| s.merchant.as_mut()) {
                    m.focused = Some(i);
                }
                self.audio
                    .play("minecraft:ui.button.click", None, 0.25, 1.0);
                self.try_move_items(index);
                return true;
            }
        }
        false
    }
    /// The offer buttons a controller can focus (GUI origin relative).
    pub(super) fn merchant_targets(&self) -> Vec<[f32; 4]> {
        let Some(m) = self.station.as_ref().and_then(|s| s.merchant.as_ref()) else {
            return Vec::new();
        };
        (0..7)
            .filter(|i| i + m.scroll < m.session.offers.len())
            .map(|i| [5.0 + SHIFT, 18.0 + 20.0 * i as f32, 88.0, 20.0])
            .collect()
    }
    pub(super) fn merchant_scroll(&mut self, d: f32) {
        let Some(m) = self.station.as_mut().and_then(|s| s.merchant.as_mut()) else {
            return;
        };
        if m.session.offers.len() > 7 {
            let max = m.session.offers.len() - 7;
            m.scroll = scrolled(m.scroll, max, d);
        }
    }
}

/// Whether a press at `local` (GUI origin relative) lands on the offer list's
/// scroller, which only takes it when there are more than 7 offers.
fn on_scroller(local: (f32, f32), offers: usize) -> bool {
    let x = SHIFT + 94.0;
    offers > 7
        && local.0 > x
        && local.0 < x + 6.0
        && local.1 > 18.0
        && local.1 <= 18.0 + 139.0 + 1.0
}
/// The first shown offer for the scroller dragged to `y` (GUI origin
/// relative): the 139-px track starts at 18 and the scroller is 27 px tall.
fn drag_scroll(y: f32, offers: usize) -> usize {
    let max = offers.saturating_sub(7) as i32;
    let offs = (y - 18.0 - 13.5) / (139.0 - 27.0);
    ((offs * max as f32 + 0.5) as i32).clamp(0, max) as usize
}

/// `MerchantScreen.mouseScrolled`: `Mth.clamp((int)(scrollOff - scrollY), 0, max)`,
/// so a fractional wheel step that truncates to zero does not move the list.
fn scrolled(scroll: usize, max: usize, d: f32) -> usize {
    ((scroll as f64 - f64::from(d)) as i32).clamp(0, max as i32) as usize
}

#[cfg(test)]
#[path = "tests/random_sequences_save_tests.rs"]
mod random_sequences_save_tests;

#[cfg(test)]
mod tests {
    use super::*;

    /// `MerchantScreen.mouseScrolled`: the offset is `(int)(scrollOff - scrollY)`
    /// clamped, so the list moves by as many rows as the event's delta and a
    /// fractional delta still moves one row (the truncation is after the
    /// subtraction). Horizon moved exactly one row per event.
    #[test]
    fn the_offers_scroll_by_the_wheel_delta() {
        assert_eq!(scrolled(3, 5, 1.0), 2, "one notch down");
        assert_eq!(scrolled(3, 5, -1.0), 4, "one notch up");
        assert_eq!(scrolled(3, 5, 0.25), 2, "a fraction still moves a row");
        assert_eq!(scrolled(3, 5, 2.5), 0, "a two-and-a-half notch event");
        assert_eq!(scrolled(0, 5, 1.0), 0, "clamped at the top");
        assert_eq!(scrolled(5, 5, -1.0), 5, "clamped at the bottom");
    }

    #[test]
    fn scroller_takes_presses_only_with_more_than_seven_offers() {
        let x = SHIFT + 97.0;
        assert!(on_scroller((x, 20.0), 8));
        assert!(!on_scroller((x, 20.0), 7));
        // The track's edges: x is exclusive both ways, y includes 158.
        assert!(!on_scroller((SHIFT + 94.0, 20.0), 8));
        assert!(!on_scroller((SHIFT + 100.0, 20.0), 8));
        assert!(!on_scroller((x, 18.0), 8));
        assert!(on_scroller((x, 158.0), 8));
        assert!(!on_scroller((x, 158.5), 8));
    }
    #[test]
    fn dragged_scroller_picks_the_nearest_offer() {
        // 10 offers: the list scrolls 3 steps over the track's 112 px.
        assert_eq!(drag_scroll(0.0, 10), 0);
        assert_eq!(drag_scroll(31.5, 10), 0);
        assert_eq!(drag_scroll(31.5 + 112.0 / 2.0, 10), 2);
        assert_eq!(drag_scroll(31.5 + 112.0, 10), 3);
        assert_eq!(drag_scroll(500.0, 10), 3);
    }

}
