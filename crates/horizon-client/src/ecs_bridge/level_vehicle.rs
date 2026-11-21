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
    /// The client moves the mount `id` (`EcsWorld::moved_by_client`).
    pub fn moved_by_client(&self, id: u32) -> bool {
        self.entity_of(id).zip(self.ecs()).is_some_and(|(e, b)| b.moved_by_client(e))
    }
}
