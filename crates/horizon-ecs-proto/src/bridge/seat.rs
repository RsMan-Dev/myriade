//! The seats of the vehicles through the bridge (`seat.rs`): what the host calls when a rider boards, leaves or is ejected. The calls that
//! may run vanilla's code take the `MobWorld` the same way `mount` does.
use crate::bridge::host::EcsWorld;
use crate::bridge::ids;
use crate::seat;
use bevy_ecs::entity::Entity;
use myriade_engine_lib::mobs::MobWorld;

impl EcsWorld {
    /// `rider` takes a seat of `vehicle` (`slot`, or the first free one); false when it already sits or the seats are taken.
    pub fn board(&mut self, mw: &MobWorld<'_>, rider: Entity, vehicle: Entity, slot: Option<u8>) -> bool {
        self.lent(mw, |w| seat::board(w, rider, vehicle, slot))
    }
    /// The creature gets off; the point of the vehicle comes back for the host's `boat_dismount_target`.
    pub fn leave(&mut self, mw: &MobWorld<'_>, rider: Entity) -> Option<[f64; 3]> {
        self.lent(mw, |w| seat::leave(w, rider))
    }
    pub fn riders_of(&self, vehicle: Entity) -> Vec<Entity> {
        seat::riders_of(self.world(), vehicle)
    }
    pub fn players_of(&self, vehicle: Entity) -> Vec<u32> {
        seat::players_of(self.world(), vehicle)
    }
    pub fn vehicle_of(&self, rider: Entity) -> Option<Entity> {
        seat::vehicle_of(self.world(), rider)
    }
    pub fn free_seat(&self, vehicle: Entity) -> bool {
        seat::free_seat(self.world(), vehicle)
    }
    /// Every rider gets off: the players first (read them with `players_of`), the creatures are returned in seat order.
    pub fn eject_all(&mut self, mw: &MobWorld<'_>, vehicle: Entity) -> Vec<Entity> {
        self.lent(mw, |w| seat::eject_all(w, vehicle))
    }
    pub fn seat_player(&mut self, player: u32, vehicle: Entity, slot: Option<u8>) -> bool {
        seat::seat_player(self.world_mut(), player, vehicle, slot)
    }
    pub fn unseat_player(&mut self, player: u32) {
        seat::unseat_player(self.world_mut(), player)
    }
    /// The network id of an entity that has one (no id is bound here).
    /// The vehicles that carry a rider: the index `RiddenVehicles` marked at each boarding (`seat::take`, `mount::ride`), pruned of the
    /// vehicles found empty or gone. Costs the number of ridden vehicles; nothing walks the mounts.
    pub fn ridden_vehicles(&mut self) -> Vec<Entity> {
        let w = self.world_mut();
        let Some(mut index) = w.remove_resource::<seat::RiddenVehicles>() else { return Vec::new() };
        index.0.retain(|&v| {
            w.get::<crate::mount::Mount>(v).is_some_and(|m| m.rider.is_some() || m.passenger.is_some())
                || w.get::<seat::Seats>(v).is_some_and(|s| s.riders.iter().any(Option::is_some))
        });
        let mut vehicles: Vec<Entity> = index.0.iter().copied().collect();
        w.insert_resource(index);
        vehicles.sort_unstable();
        vehicles
    }
    /// The creature riders of `vehicle` as `Passengers` writes them: its seats, then its mount's riders (`rides::passengers_of`).
    pub fn passengers_of(&self, vehicle: Entity) -> Vec<Entity> {
        crate::nbt_fields::rides::passengers_of(self.world(), vehicle)
    }
    pub fn id_of(&self, e: Entity) -> Option<u32> {
        ids::id_of(self.world(), e)
    }
}
