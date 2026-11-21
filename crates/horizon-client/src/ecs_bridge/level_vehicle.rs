//! The vehicles, the seats, the mounts' slots and the trade of the ECS entities, by network id (lot 45): each call reads or writes the
//! bridge's `EcsWorld` on the id's entity (bridge/host_vehicle.rs). An inert level answers as an empty world: none, false, nothing.
use super::level::EcsLevel;
use bevy_ecs::entity::Entity;
use horizon_client_lib::{items::ItemStack, merchant::MerchantOffer};
use horizon_ecs_proto::bridge::ids::id_of;

impl EcsLevel {
    /// The id of the vehicle `id` rides, None when it rides nothing.
    pub fn host_vehicle(&self, id: u32) -> Option<u32> {
        let v = self.ecs()?.host_vehicle(self.entity_of(id)?)?;
        id_of(self.world()?, v)
    }
    /// The ids of the creatures that ride the vehicle `cid`.
    pub fn host_riders(&self, cid: u32) -> Vec<u32> {
        let (Some(w), Some(b), Some(e)) = (self.world(), self.ecs(), self.entity_of(cid)) else { return Vec::new() };
        b.host_riders(e).into_iter().filter_map(|r| id_of(w, r)).collect()
    }
    /// The id of the vehicle of the root `root` (`EcsWorld::vehicle_id`).
    pub fn vehicle_id(&self, root: u32) -> Option<u32> {
        self.ecs()?.vehicle_id(self.entity_of(root)?)
    }
    pub fn is_passenger(&self, id: u32) -> bool {
        self.entity_of(id).zip(self.ecs()).is_some_and(|(e, b)| b.is_passenger(e))
    }
    pub fn has_free_seat(&self, id: u32) -> bool {
        self.entity_of(id).zip(self.ecs()).is_some_and(|(e, b)| b.has_free_seat(e))
    }
    /// The riders of `from` ride `to` (`EcsWorld::take_passengers`).
    pub fn take_passengers(&mut self, from: u32, to: u32) {
        let (Some(f), Some(t)) = (self.entity_of(from), self.entity_of(to)) else { return };
        if let Some(b) = self.ecs_mut() {
            b.take_passengers(f, t);
        }
    }
    /// `e` rides the vehicle `cid` at `seat` (`EcsWorld::seat_in_host_vehicle`).
    pub fn seat_in_host_vehicle(&mut self, e: Entity, cid: u32, seat: usize) -> bool {
        self.ecs_mut().is_some_and(|b| b.seat_in_host_vehicle(e, cid, seat))
    }
    pub fn boarding_cooldown(&self, e: Entity) -> Option<u64> {
        self.ecs()?.boarding_cooldown(e)
    }
    pub fn boarding_ready(&self, e: Entity) -> bool {
        self.ecs().is_some_and(|b| b.boarding_ready(e))
    }
    /// The columns of the mount `id`'s inventory (`EcsWorld::mount_columns`).
    pub fn mount_columns(&self, id: u32) -> Option<usize> {
        self.ecs()?.mount_columns(self.entity_of(id)?).map(|c| c.columns)
    }
    /// The saddle, the armour and the items of the mount `id` (`EcsWorld::mount_contents`).
    pub fn mount_contents(&self, id: u32) -> Option<(Option<ItemStack>, Option<Option<ItemStack>>, Vec<Option<ItemStack>>)> {
        let c = self.ecs()?.mount_contents(self.entity_of(id)?)?;
        Some((c.saddle, c.body, c.items))
    }
    /// Writes the saddle, the armour and the items of the mount `id` (`EcsWorld::mount_store`).
    pub fn mount_store(&mut self, id: u32, saddle: Option<ItemStack>, body: Option<Option<ItemStack>>, items: Vec<Option<ItemStack>>) {
        let Some(e) = self.entity_of(id) else { return };
        if let Some(b) = self.ecs_mut() {
            b.mount_store(e, saddle, body, items);
        }
    }
    /// A villager with no profession: its trade screen is opaque (`EcsWorld::trader_opaque`).
    pub fn trader_opaque(&self, id: u32) -> bool {
        self.entity_of(id).and_then(|e| self.ecs()?.trader_opaque(e)).unwrap_or(false)
    }
    pub fn ensure_offers(&mut self, id: u32) {
        let Some(e) = self.entity_of(id) else { return };
        if let Some(b) = self.ecs_mut() {
            b.ensure_offers(e);
        }
    }
    pub fn trade_offers(&self, id: u32) -> Option<Vec<MerchantOffer>> {
        self.ecs()?.trade_offers(self.entity_of(id)?)
    }
    /// The offers of the merchant `id` are these (`EcsWorld::set_trade_offers`).
    pub fn set_trade_offers(&mut self, id: u32, offers: Vec<MerchantOffer>) {
        let Some(e) = self.entity_of(id) else { return };
        if let Some(b) = self.ecs_mut() {
            b.set_trade_offers(e, offers);
        }
    }
    /// `EcsWorld::install_trades`.
    pub fn install_trades(
        &mut self,
        trades: crate::merchant::Trades,
        items: &'static crate::items::Items,
        enchantments: &'static horizon_items::enchant::Enchantments,
        seed: i64,
    ) {
        if let Some(b) = self.ecs_mut() {
            b.install_trades(trades, items, enchantments, seed);
        }
    }
    pub fn trade_open(&mut self, id: u32, player: u32) {
        let Some(e) = self.entity_of(id) else { return };
        if let Some(b) = self.ecs_mut() {
            b.trade_open(e, player);
        }
    }
    pub fn trade_close(&mut self, id: u32) {
        let Some(e) = self.entity_of(id) else { return };
        if let Some(b) = self.ecs_mut() {
            b.trade_close(e);
        }
    }
    pub fn trade_busy(&self, id: u32) -> bool {
        self.entity_of(id).zip(self.ecs()).is_some_and(|(e, b)| b.trade_busy(e))
    }
    /// Some(true) for a villager, Some(false) for a wandering trader, None without a merchant (`EcsWorld::is_villager`).
    pub fn is_villager(&self, id: u32) -> Option<bool> {
        self.ecs()?.is_villager(self.entity_of(id)?)
    }
    /// The merchant's side of the open screen of `id` (`EcsWorld::trade_session`).
    pub fn merchant_session(&self, id: u32) -> Option<horizon_client_lib::merchant::Session> {
        self.ecs()?.trade_session(self.entity_of(id)?)
    }
    /// The offers and the experience of the merchant `id` (`EcsWorld::trade_store`).
    pub fn trade_store(&mut self, id: u32, offers: Vec<MerchantOffer>, xp: i32) {
        let Some(e) = self.entity_of(id) else { return };
        if let Some(b) = self.ecs_mut() {
            b.trade_store(e, offers, xp);
        }
    }
    pub fn trade_random(&self, id: u32) -> Option<horizon_core::random::LegacyRandom> {
        self.ecs()?.trade_random(self.entity_of(id)?)
    }
    pub fn set_trade_random(&mut self, id: u32, random: horizon_core::random::LegacyRandom) {
        let Some(e) = self.entity_of(id) else { return };
        if let Some(b) = self.ecs_mut() {
            b.set_trade_random(e, random);
        }
    }
    pub fn ambient_timer(&self, id: u32) -> Option<i32> {
        self.ecs()?.ambient_timer(self.entity_of(id)?)
    }
    pub fn set_ambient_timer(&mut self, id: u32, time: i32) {
        let Some(e) = self.entity_of(id) else { return };
        if let Some(b) = self.ecs_mut() {
            b.set_ambient_timer(e, time);
        }
    }
    pub fn set_trade_level(&mut self, id: u32, level: i32) {
        let Some(e) = self.entity_of(id) else { return };
        if let Some(b) = self.ecs_mut() {
            b.set_trade_level(e, level);
        }
    }
    /// `Villager.getVillagerXp` (`EcsWorld::trade_xp`).
    pub fn trade_xp(&self, id: u32) -> Option<i32> {
        self.ecs()?.trade_xp(self.entity_of(id)?)
    }
    pub fn set_trade_xp(&mut self, id: u32, xp: i32) {
        let Some(e) = self.entity_of(id) else { return };
        if let Some(b) = self.ecs_mut() {
            b.set_trade_xp(e, xp);
        }
    }
    /// `VillagerData.level` (`EcsWorld::trade_level`).
    pub fn trade_level(&self, id: u32) -> Option<i32> {
        self.ecs()?.trade_level(self.entity_of(id)?)
    }
    /// `VillagerData.type`, a registry id (`EcsWorld::trade_variant`).
    pub fn trade_variant(&self, id: u32) -> Option<String> {
        self.ecs()?.trade_variant(self.entity_of(id)?)
    }
    pub fn set_trade_variant(&mut self, id: u32, variant: u8) {
        let Some(e) = self.entity_of(id) else { return };
        if let Some(b) = self.ecs_mut() {
            b.set_trade_variant(e, variant);
        }
    }
    /// `VillagerData.profession`, a registry id; None for a wandering trader (`EcsWorld::trade_profession`).
    pub fn trade_profession(&self, id: u32) -> Option<String> {
        self.ecs()?.trade_profession(self.entity_of(id)?)
    }
    pub fn set_trade_profession(&mut self, id: u32, index: u8) {
        let Some(e) = self.entity_of(id) else { return };
        if let Some(b) = self.ecs_mut() {
            b.set_trade_profession(e, index);
        }
    }
    /// `Villager.setUnhappy` (`EcsWorld::set_unhappy`).
    pub fn set_unhappy(&mut self, id: u32) {
        let Some(e) = self.entity_of(id) else { return };
        if let Some(b) = self.ecs_mut() {
            b.set_unhappy(e);
        }
    }
    /// The client moves the mount `id` (`EcsWorld::moved_by_client`).
    pub fn moved_by_client(&self, id: u32) -> bool {
        self.entity_of(id).zip(self.ecs()).is_some_and(|(e, b)| b.moved_by_client(e))
    }
}
