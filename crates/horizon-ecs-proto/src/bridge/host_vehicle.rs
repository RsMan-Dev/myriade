//! The vehicles and the trade of the host's entities, by `Entity` (greffe lot 45). A rider is `Riding` on its mount, a mount keeps its seats
//! in `Mount` (the first rider steers), its saddle and armour are its `Equipment`, and a villager's offers are its `Merchant`. Everything is
//! read through those relations, never by a scan; a write marks the views it changes. A vehicle of the host is an ECS entity too.
use crate::behaviours::{trade_offers, trade_session};
use crate::bridge::{host::EcsWorld, ids, view};
use crate::cow::now_of;
use crate::equipment::set_slot;
use crate::goals::Slot;
use crate::items::item_id;
use crate::mount::{self, Mount, Riding};
use crate::targeting::Who;
use bevy_ecs::prelude::*;
use horizon_client_lib::{items::ItemStack, merchant::MerchantOffer};

/// The columns of a mount's inventory. P keeps no inventory of a mount, so no mount has columns yet.
pub struct MountColumns {
    pub columns: usize,
}

/// A mount's slots as its menu shows them: the saddle, the armour (`None` for a mount that has no armour slot), and the chest's items.
pub struct MountContents {
    pub saddle: Option<ItemStack>,
    pub body: Option<Option<ItemStack>>,
    pub items: Vec<Option<ItemStack>>,
}

/// The item id of a slot's stack (0: empty).
fn id_of_stack(s: &Option<ItemStack>) -> u32 {
    s.as_ref().map_or(0, |s| item_id(&s.item))
}

impl EcsWorld {
    /// The vehicle `e` rides (`Riding`), if any.
    pub fn host_vehicle(&self, e: Entity) -> Option<Entity> {
        self.world().get::<Riding>(e).map(|r| r.0)
    }

    /// The creature riders of `vehicle`, its first seat then its second. A player rides by its id, so it is not listed here.
    pub fn host_riders(&self, vehicle: Entity) -> Vec<Entity> {
        let Some(m) = self.world().get::<Mount>(vehicle) else { return Vec::new() };
        [m.rider, m.passenger]
            .into_iter()
            .flatten()
            .filter_map(|who| match who {
                Who::Entity(e) => Some(e),
                Who::Player(_) => None,
            })
            .collect()
    }

    /// The network id of the vehicle of `root` (`NetId`), None when it rides nothing or its vehicle has no id.
    pub fn vehicle_id(&self, root: Entity) -> Option<u32> {
        ids::id_of(self.world(), self.host_vehicle(root)?)
    }

    pub fn is_passenger(&self, e: Entity) -> bool {
        self.world().get::<Riding>(e).is_some()
    }

    /// A seat of `vehicle` is free: the first seat, or the second one of a mount that has two.
    pub fn has_free_seat(&self, vehicle: Entity) -> bool {
        self.world().get::<Mount>(vehicle).is_some_and(|m| m.rider.is_none() || (m.seats.len() > 1 && m.passenger.is_none()))
    }

    /// Every rider of `from` gets off and rides `to` in seat order; a rider `to` has no seat for is set down. The views of the three move.
    pub fn take_passengers(&mut self, from: Entity, to: Entity) {
        let w = self.world_mut();
        let mut riders = Vec::new();
        while let Some(who) = mount::dismount(w, from) {
            riders.push(who);
        }
        for who in riders {
            mount::ride(w, to, who);
            if let Who::Entity(r) = who {
                view::mark(w, r);
            }
        }
        view::mark(w, from);
        view::mark(w, to);
    }

    /// `e` rides the vehicle `vehicle_id` of the host at `seat`, the next free one (the seats fill in order). False when `seat` is not
    /// the next one, or the vehicle has no `Mount` (a boat: its seats are not modelled yet).
    pub fn seat_in_host_vehicle(&mut self, e: Entity, vehicle_id: u32, seat: usize) -> bool {
        let Some(v) = self.entity_of(vehicle_id) else { return false };
        let taken = self.world().get::<Mount>(v).map_or(0, |m| usize::from(m.rider.is_some()) + usize::from(m.passenger.is_some()));
        seat == taken && mount::ride(self.world_mut(), v, Who::Entity(e))
    }

    /// The ticks left before `e` may board again (`Mount.ready_at` against its own time); None when it may, or has no mount.
    pub fn boarding_cooldown(&self, e: Entity) -> Option<u64> {
        let w = self.world();
        let ready = w.get::<Mount>(e)?.ready_at;
        let now = now_of(w, e);
        (ready > now).then(|| ready - now)
    }

    pub fn boarding_ready(&self, e: Entity) -> bool {
        self.world().get::<Mount>(e).is_some() && self.boarding_cooldown(e).is_none()
    }

    /// The columns of the mount's chest: a horse (`max_temper` above 0) has a menu, with no column unless it carries a chest. None otherwise.
    pub fn mount_columns(&self, e: Entity) -> Option<MountColumns> {
        let w = self.world();
        if w.get::<Mount>(e)?.max_temper <= 0 {
            return None;
        }
        Some(MountColumns { columns: w.get::<crate::mount_inventory::MountInventory>(e).map_or(0, |c| usize::from(c.columns)) })
    }

    /// The saddle and, for a horse (`max_temper` is 0 for a mount that is not an `AbstractHorse`), the armour, and the chest's items (none without a chest).
    pub fn mount_contents(&self, e: Entity) -> Option<MountContents> {
        let w = self.world();
        let m = w.get::<Mount>(e)?;
        let saddle = crate::equip_stack::get(w, e, 7);
        let body = (m.max_temper > 0).then(|| crate::equip_stack::get(w, e, 6));
        let items = w.get::<crate::mount_inventory::MountInventory>(e).map_or_else(Vec::new, |c| c.items.clone());
        Some(MountContents { saddle, body, items })
    }

    /// Puts the saddle, and the armour when given, in the mount's slots (`saddled` follows the saddle), and the items in its chest when it has one.
    pub fn mount_store(&mut self, e: Entity, saddle: Option<ItemStack>, body: Option<Option<ItemStack>>, items: Vec<Option<ItemStack>>) {
        let w = self.world_mut();
        set_slot(w, e, Slot::Saddle, id_of_stack(&saddle));
        crate::equip_stack::set(w, e, 7, saddle);
        if let Some(b) = body {
            set_slot(w, e, Slot::Body, id_of_stack(&b));
            crate::equip_stack::set(w, e, 6, b);
        }
        crate::mount_inventory::set_items(w, e, items);
        view::mark(w, e);
    }

    /// A villager with no profession: its trade screen is opaque to a guest. A wandering trader never is. None without a `Merchant`.
    pub fn trader_opaque(&self, e: Entity) -> Option<bool> {
        let w = self.world();
        let m = w.get::<trade_offers::Merchant>(e)?;
        Some(!m.wandering && trade_offers::profession_name(w, m.profession).is_empty())
    }

    /// `Villager.getOffers`: the offers are made when they are not yet.
    pub fn ensure_offers(&mut self, e: Entity) {
        trade_offers::ensure_offers(self.world_mut(), e);
    }

    /// The offers of the merchant `e` as they stand (none without a `Merchant`).
    pub fn trade_offers(&self, e: Entity) -> Option<Vec<MerchantOffer>> {
        let w = self.world();
        if let Some(t) = w.get::<trade_session::Trading>(e) {
            return Some(t.session.offers.clone());
        }
        w.get::<trade_offers::Merchant>(e).map(|m| m.trade.offers.clone())
    }

    /// Sets the offers of the merchant `e`, as made: they are not remade by `ensure_offers`.
    pub fn set_trade_offers(&mut self, e: Entity, offers: Vec<MerchantOffer>) {
        if let Some(mut m) = self.world_mut().get_mut::<trade_offers::Merchant>(e) {
            m.trade.offers = offers;
            m.trade.unset = false;
        }
    }

    /// The trade registries of the world (`TradeData`), installed once by the host: offers, professions (a mod's included, the same
    /// `ResolvedProfessions` the host resolves ids with) and the sequences' seed.
    pub fn install_trades(
        &mut self,
        trades: horizon_client_lib::merchant::Trades,
        items: &'static horizon_client_lib::items::Items,
        enchantments: &'static horizon_items::enchant::Enchantments,
        seed: i64,
    ) {
        self.world_mut().insert_resource(trade_offers::TradeData { trades, items, enchantments, seed, sequences: Default::default() });
    }

    /// `player` opens the trade screen of `e` (`trade_session::open`).
    pub fn trade_open(&mut self, e: Entity, player: u32) {
        trade_session::open(self.world_mut(), e, player);
    }

    /// `player` closes the trade screen of `e` (`trade_session::close`).
    pub fn trade_close(&mut self, e: Entity) {
        trade_session::close(self.world_mut(), e);
    }

    /// A trade screen is open on `e`.
    pub fn trade_busy(&self, e: Entity) -> bool {
        self.world().get::<trade_session::Trading>(e).is_some()
    }

    /// Whether the merchant `e` is a villager (not a wandering trader); None without a `Merchant`.
    pub fn is_villager(&self, e: Entity) -> Option<bool> {
        self.world().get::<trade_offers::Merchant>(e).map(|m| !m.wandering)
    }

    /// The merchant's side of the open screen of `e` (`Trading.session`); None while no screen is open.
    pub fn trade_session(&self, e: Entity) -> Option<horizon_client_lib::merchant::Session> {
        self.world().get::<trade_session::Trading>(e).map(|t| t.session.clone())
    }


    /// Writes the offers and the experience of the merchant `e`: into the open screen when there is one, else into the `Merchant`.
    pub fn trade_store(&mut self, e: Entity, offers: Vec<MerchantOffer>, xp: i32) {
        let w = self.world_mut();
        if let Some(mut t) = w.get_mut::<trade_session::Trading>(e) {
            t.session.offers = offers;
            t.session.xp = xp;
        } else if let Some(mut m) = w.get_mut::<trade_offers::Merchant>(e) {
            m.trade.offers = offers;
            m.trade.xp = xp;
            m.trade.unset = false;
        }
    }

    /// The merchant's own random of the open screen (`rewardTradeXp`); None while no screen is open.
    pub fn trade_random(&self, e: Entity) -> Option<horizon_core::random::LegacyRandom> {
        self.world().get::<trade_session::Trading>(e).map(|t| t.session.random.clone())
    }

    pub fn set_trade_random(&mut self, e: Entity, random: horizon_core::random::LegacyRandom) {
        if let Some(mut t) = self.world_mut().get_mut::<trade_session::Trading>(e) {
            t.session.random = random;
        }
    }

    /// `Mob.ambientSoundTime` of the open screen; None while no screen is open.
    pub fn ambient_timer(&self, e: Entity) -> Option<i32> {
        self.world().get::<trade_session::Trading>(e).map(|t| t.session.ambient_sound_time)
    }

    pub fn set_ambient_timer(&mut self, e: Entity, time: i32) {
        if let Some(mut t) = self.world_mut().get_mut::<trade_session::Trading>(e) {
            t.session.ambient_sound_time = time;
        }
    }

    /// The villager level (1 to 5) of the merchant `e`: in the open screen when there is one, else in the `Merchant`.
    pub fn set_trade_level(&mut self, e: Entity, level: i32) {
        let w = self.world_mut();
        if let Some(mut t) = w.get_mut::<trade_session::Trading>(e) {
            t.session.level = level;
        }
        if let Some(mut m) = w.get_mut::<trade_offers::Merchant>(e) {
            m.level = u8::try_from(level).unwrap_or(m.level);
        }
    }

    /// `Villager.getVillagerXp`: the open screen's when there is one, else the `Merchant`'s.
    pub fn trade_xp(&self, e: Entity) -> Option<i32> {
        let w = self.world();
        w.get::<trade_session::Trading>(e).map(|t| t.session.xp).or_else(|| w.get::<trade_offers::Merchant>(e).map(|m| m.trade.xp))
    }

    /// `Villager.setVillagerXp`, assigned as given.
    pub fn set_trade_xp(&mut self, e: Entity, xp: i32) {
        let w = self.world_mut();
        if let Some(mut t) = w.get_mut::<trade_session::Trading>(e) {
            t.session.xp = xp;
        }
        if let Some(mut m) = w.get_mut::<trade_offers::Merchant>(e) {
            m.trade.xp = xp;
        }
    }

    /// `VillagerData.level` (1 for a wandering trader): the open screen's when there is one.
    pub fn trade_level(&self, e: Entity) -> Option<i32> {
        let w = self.world();
        w.get::<trade_session::Trading>(e).map(|t| t.session.level).or_else(|| w.get::<trade_offers::Merchant>(e).map(|m| i32::from(m.level)))
    }

    /// `VillagerData.type`, a registry id; None without a `Merchant`.
    pub fn trade_variant(&self, e: Entity) -> Option<String> {
        self.world().get::<trade_offers::Merchant>(e).map(|m| trade_offers::variant_name(m.variant))
    }

    /// `VillagerData.withType`: the index in `VILLAGER_TYPES`.
    pub fn set_trade_variant(&mut self, e: Entity, variant: u8) {
        if let Some(mut m) = self.world_mut().get_mut::<trade_offers::Merchant>(e) {
            m.variant = variant;
        }
    }

    /// `VillagerData.profession`, a registry id; None for a wandering trader or without a `Merchant`.
    pub fn trade_profession(&self, e: Entity) -> Option<String> {
        let w = self.world();
        let m = w.get::<trade_offers::Merchant>(e).filter(|m| !m.wandering)?;
        Some(match w.get::<trade_offers::UnresolvedProfession>(e) {
            Some(u) => u.0.clone(),
            None => trade_offers::profession_name(w, m.profession),
        })
    }

    /// `Villager.setVillagerData` with the profession at `index` of the active professions: a different one forgets the offers.
    pub fn set_trade_profession(&mut self, e: Entity, index: u8) {
        trade_offers::set_profession(self.world_mut(), e, index);
    }

    /// `Villager.setUnhappy`: the villager says no (a baby with the pitch of its age) and shakes its head (`behaviours::unhappy`).
    pub fn set_unhappy(&mut self, e: Entity) {
        let w = self.world_mut();
        let Some(m) = w.get::<trade_offers::Merchant>(e) else { return };
        let wandering = m.wandering;
        crate::behaviours::unhappy::start(w, e);
        let name = if wandering { "entity.wandering_trader.no" } else { "entity.villager.no" };
        let pitch = if w.get::<crate::cow::GrowAt>(e).is_some() { 1.5 } else { 1.0 };
        let at = w.get::<crate::cow::Pos>(e).map_or([0.0; 3], |p| [p.0.x, p.0.y, p.0.z]);
        w.resource_mut::<crate::goals::Events>().host.sound(name, at, 1.0, pitch);
    }

    /// A mount its rider's client moves (`getControllingPassenger` is a player, `handleMoveVehicle`): the one `player_steers` says the
    /// player steers. The mount's own `Mount` rider is the only state; no species.
    pub fn moved_by_client(&self, e: Entity) -> bool {
        self.player_steers(e)
    }
}
