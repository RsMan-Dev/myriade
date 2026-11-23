//! The seats of the vehicles on the server's side (`P bridge/seat.rs`): ids in, the bridge's calls out. An inert level (a client) answers as
//! an empty world: nothing boards, nothing is riding.
use super::level::EcsLevel;
use crate::mobs::MobWorld;

impl EcsLevel {
    /// The creature `rider` takes a seat of `vehicle` (both ids); false when either is not in the level or the seats are taken.
    pub fn board(&mut self, mw: &MobWorld<'_>, rider: u32, vehicle: u32, slot: Option<u8>) -> bool {
        let (Some(r), Some(v)) = (self.entity_of(rider), self.entity_of(vehicle)) else { return false };
        self.ecs_mut().is_some_and(|w| w.board(mw, r, v, slot))
    }
    pub fn leave(&mut self, mw: &MobWorld<'_>, rider: u32) -> Option<[f64; 3]> {
        let r = self.entity_of(rider)?;
        self.ecs_mut()?.leave(mw, r)
    }
    /// The creatures of `vehicle`, in seat order, by id.
    pub fn riders_of(&self, vehicle: u32) -> Vec<u32> {
        let (Some(v), Some(w)) = (self.entity_of(vehicle), self.ecs()) else { return Vec::new() };
        w.riders_of(v).into_iter().filter_map(|e| w.id_of(e)).collect()
    }
    /// The players of `vehicle`, in seat order (their ids are the host's).
    pub fn players_of(&self, vehicle: u32) -> Vec<u32> {
        let (Some(v), Some(w)) = (self.entity_of(vehicle), self.ecs()) else { return Vec::new() };
        w.players_of(v)
    }
    /// The ids of the vehicles that carry a creature (a seat or a mount).
    pub fn ridden_vehicles(&mut self) -> Vec<u32> {
        let Some(w) = self.ecs_mut() else { return Vec::new() };
        let vehicles = w.ridden_vehicles();
        vehicles.into_iter().filter_map(|e| w.id_of(e)).collect()
    }
    /// The creatures riding `vehicle` (seats, then mount), the mount's driver first: the order `Entity.getPassengers` gives.
    pub fn passengers_of(&self, vehicle: u32) -> Vec<u32> {
        let (Some(v), Some(w)) = (self.entity_of(vehicle), self.ecs()) else { return Vec::new() };
        let driver = match horizon_ecs_proto::mount::rider_of(w.world(), v) {
            Some(horizon_ecs_proto::targeting::Who::Entity(d)) => Some(d),
            _ => None,
        };
        let mut riders = w.passengers_of(v);
        if let Some(i) = driver.and_then(|d| riders.iter().position(|&r| r == d)) {
            let d = riders.remove(i);
            riders.insert(0, d);
        }
        riders.into_iter().filter_map(|e| w.id_of(e)).collect()
    }
    pub fn vehicle_of(&self, rider: u32) -> Option<u32> {
        let w = self.ecs()?;
        let v = w.vehicle_of(self.entity_of(rider)?)?;
        w.id_of(v)
    }
    pub fn free_seat(&self, vehicle: u32) -> bool {
        match (self.entity_of(vehicle), self.ecs()) {
            (Some(v), Some(w)) => w.free_seat(v),
            _ => false,
        }
    }
    /// Every creature of `vehicle` gets off: its ids come back in seat order. Read `players_of` first: the players are unseated here.
    pub fn eject_all(&mut self, mw: &MobWorld<'_>, vehicle: u32) -> Vec<u32> {
        let Some(v) = self.entity_of(vehicle) else { return Vec::new() };
        let Some(w) = self.ecs_mut() else { return Vec::new() };
        let ejected = w.eject_all(mw, v);
        ejected.into_iter().filter_map(|e| w.id_of(e)).collect()
    }
    pub fn seat_player(&mut self, player: u32, vehicle: u32, slot: Option<u8>) -> bool {
        let Some(v) = self.entity_of(vehicle) else { return false };
        self.ecs_mut().is_some_and(|w| w.seat_player(player, v, slot))
    }
    pub fn unseat_player(&mut self, player: u32) {
        if let Some(w) = self.ecs_mut() {
            w.unseat_player(player);
        }
    }
}
