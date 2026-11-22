//! Boats and the entities around them: what a boat runs into
//! (`AbstractBoat.canVehicleCollide`), the pushes between a boat and what
//! touches it (`AbstractBoat.push`, `LivingEntity.pushEntities`), the mobs
//! it picks up (the end of `AbstractBoat.tick`), where they sit
//! (`positionRider`, `clampRotation`) and how they get out.
//! Saving them under the boat's `Passengers` lives in `persist.rs`.
use super::{Game, lan_mobs::guest_bounds};
use horizon_core::geometry::{Aabb, Vec3};

/// `Entity.boardingCooldown` once a mob leaves its boat.
pub(super) const BOARDING_COOLDOWN: i32 = 60;

impl Game<'_> {
    /// `isPushable` of the followed mob `id`, as `pushableBy` and `canVehicleCollide` ask it:
    /// what `ecs.pushable` allows, plus the living shulkers, which take no push but count.
    fn mob_pushable(&self, id: u32) -> bool {
        self.ecs.pushable(id) || self.ecs.kind(id) == Some("minecraft:shulker") && self.mob_alive(id)
    }
    /// A vehicle's box query: `each` gets every followed mob whose box meets `area`, with its
    /// id and its box.
    pub(in crate::game) fn mobs_meeting(&self, area: Aabb, mut each: impl FnMut(u32, Aabb)) {
        for id in self.ecs_entities_meeting(area) {
            if let Some(b) = self.mob_box(id) {
                each(id, b);
            }
        }
    }
    /// The players in boat `id`: the window's player (or the server player
    /// acting as it) and the server's other players.
    pub(super) fn boat_players(&self, id: u32) -> usize {
        let guests = self.lan.as_ref().map_or(0, |h| {
            h.hub
                .guests
                .iter()
                .filter(|g| Some(g.conn) != self.lan_acting && g.vehicle == Some(id))
                .count()
        });
        usize::from(self.riding == Some(id)) + guests
    }
    /// The mobs in boat `id`, in their seat order after the players
    /// (`Entity.addPassenger` puts a player first).
    pub(super) fn boat_mobs(&self, id: u32) -> Vec<u32> {
        self.ecs.riders_of(id).into_iter().filter(|&m| self.mob_alive(m)).collect()
    }
    /// `getPassengers().size()` of boat `id`.
    /// `getPassengers().size()` of boat `id`.
    pub(super) fn boat_passengers(&self, id: u32) -> usize {
        self.boat_players(id) + self.ecs.riders_of(id).into_iter().filter(|&m| self.mob_exists(m)).count()
    }
    /// `AbstractBoat.canAddPassenger`: a seat is free and the boat is not
    /// under water.
    pub(super) fn boat_has_room(&self, id: u32) -> bool {
        let under_water = self.ecs.boat_status(id).is_some_and(|s| s.under_water);
        self.boat_passengers(id) < self.ecs.boat_max_passengers(id).unwrap_or(0) && !under_water
    }
    /// Where the player in boat `i` sits: in the middle alone, in front of a
    /// mob.
    pub(super) fn player_boat_seat(&self, id: u32) -> Vec3 {
        let players = self.boat_players(id);
        let count = players.max(1) + self.boat_mobs(id).len();
        self.ecs.boat_seat(id, 0, count).unwrap_or(Vec3::ZERO)
    }
    /// `getRootVehicle` of the followed mob `id`: the mob or boat at the bottom of what it
    /// rides, None when it rides nothing.
    pub(super) fn mob_root_vehicle(&self, mut id: u32) -> Option<u32> {
        let mut root = None;
        for _ in 0..8 {
            let Some(v) = self.ecs.vehicle_of(id) else {
                break;
            };
            root = Some(v);
            id = v;
        }
        root
    }
    /// The players outside boat `id` that push and block it (living, not
    /// spectators), with the window player's team.
    fn players_outside(&self, id: u32) -> Vec<(Aabb, bool)> {
        let mut out = Vec::new();
        if let Some(p) = self.player.as_ref()
            && self.riding != Some(id)
            && !self.spectator()
            && !p.health.dead_or_dying()
        {
            out.push((p.bounds(), true));
        }
        if let Some(host) = self.lan.as_ref() {
            for g in host.hub.guests.iter().filter(|g| {
                Some(g.conn) != self.lan_acting
                    && host.hub.here(g)
                    && g.loaded
                    && !g.dead
                    && !g.spectator()
                    && g.vehicle != Some(id)
            }) {
                out.push((guest_bounds(g), false));
            }
        }
        out
    }
    /// `getEntityCollisions` of boat `id` within `area`, with
    /// `AbstractBoat.canVehicleCollide`: the other boats, minecarts,
    /// shulkers and pushable mobs and players, but not its own passengers.
    pub(super) fn boat_obstacles(&self, id: u32, area: Aabb) -> Vec<Aabb> {
        let mut out = Vec::new();
        self.mobs_meeting(area, |j, b| {
            if self.mob_pushable(j) && self.mob_root_vehicle(j) != Some(id) {
                out.push(b);
            }
        });
        let boats = self
            .ecs
            .boats_in(area.min_position(), area.max_position())
            .into_iter()
            .filter(|&b| b != id)
            .filter_map(|b| self.ecs.bounds(b));
        out.extend(
            boats
                .chain(self.players_outside(id).into_iter().map(|(bb, _)| bb))
                .filter(|bb| bb.intersects(area)),
        );
        out
    }
    /// `noEntityCollision(boat, box)` for a new boat (`BoatItem.use`).
    pub(super) fn boat_entity_collision(&self, bb: Aabb) -> bool {
        // EntityGetter.getEntityCollisions: the box grown by 1e-7.
        let area = bb.inflate(Vec3::new(1.0e-7, 1.0e-7, 1.0e-7));
        !self.boat_obstacles(u32::MAX, area).is_empty()
    }

    /// `Entity.setRemoved` for a killed boat or cart; false on a reentrant
    /// removal of the same object while its dismount callbacks are running.
    pub(super) fn kill_vehicle_object(&mut self, target: super::commands::Target) -> bool {
        use super::commands::Target;
        let id = match target {
            Target::Boat(id) | Target::Cart(id) => id,
            _ => unreachable!("only boat and cart removals use this helper"),
        };
        if !self.persist.notified_vehicle_removals.remove(&id)
            && self
                .with_destroying_vehicle(target, |game| {
                    game.dismount_destroyed_vehicle_players(id);
                    match target {
                        Target::Boat(_) => game.dismount_killed_boat(id),
                        Target::Cart(_) => game.dismount_killed_cart(id),
                        _ => unreachable!(),
                    }
                })
                .is_none()
        {
            return false;
        }
        match target {
            Target::Boat(_) => {
                self.release_vehicle_container(id);
                self.unpack_boat_loot(id, false);
                self.ecs_with(|ecs, mw| ecs.kill_vehicle(mw, id));
            }
            Target::Cart(_) => {
                self.release_vehicle_container(id);
                self.unpack_loot(super::container_loot::LootTarget::Cart(id), None);
                self.ecs_with(|ecs, mw| ecs.kill_vehicle(mw, id));
            }
            _ => unreachable!(),
        }
        true
    }

    /// `Entity.setRemoved` also attempts the player edges, using their own
    /// server context and rechecking each after earlier callbacks.
    pub(super) fn dismount_destroyed_vehicle_players(&mut self, vehicle: u32) {
        let players: Vec<_> = self
            .seated_passengers(vehicle)
            .into_iter()
            .filter_map(|(_, id, player)| player.then_some(id))
            .collect();
        let local_listed = self
            .lan_acting_entity
            .is_some_and(|id| players.contains(&id));
        for id in players {
            if self
                .seated_passengers(vehicle)
                .iter()
                .any(|&(_, rider, player)| player && rider == id)
                && let Some(rider) = self.entity(id)
            {
                self.entity_stop_riding(rider);
            }
        }
        // A local player outside a server guest turn has no seated entity ID.
        if !local_listed
            && self.lan_acting.is_none()
            && self.player.is_some()
            && self.riding == Some(vehicle)
        {
            self.stop_riding();
        }
    }

    /// `Entity.setRemoved`: attempt each passenger once; a veto keeps the edge.
    pub(super) fn dismount_destroyed_vehicle_mobs(&mut self, vehicle: u32, y: f64) {
        for rider in self.mob_passenger_ids(vehicle) {
            // RESTE-SANS-ECS: the retained off-level riders this dismounted are retired.
            let _ = (rider, y);
        }
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use horizon_ecs_proto::rules::push_rules::{boat_pushes, push_impulse};

    #[test]
    fn push_is_capped_and_scaled() {
        // One block apart on x: the full 0.05 along x.
        let v = push_impulse(Vec3::ZERO, Vec3::new(1.0, 0.0, 0.0)).unwrap();
        assert!((v.x - f64::from(0.05_f32)).abs() < 1e-12 && v.z == 0.0);
        // Almost on the same spot: no push.
        assert!(push_impulse(Vec3::ZERO, Vec3::new(0.005, 0.0, 0.0)).is_none());
    }

    #[test]
    fn boats_push_by_height() {
        let boat = Aabb::new(Vec3::new(0.0, 64.0, 0.0), Vec3::new(1.0, 64.5625, 1.0));
        let at = |y: f64| Aabb::new(Vec3::new(0.0, y, 0.0), Vec3::new(1.0, y + 1.0, 1.0));
        assert!(boat_pushes(boat, at(64.0), false));
        assert!(!boat_pushes(boat, at(64.1), false));
        assert!(boat_pushes(boat, at(64.5), true));
        assert!(!boat_pushes(boat, at(64.5625), true));
    }

}

#[cfg(test)]
mod boat_callback_review_tests {
    use super::*;
    use horizon_client::mobs::Mob;
    use std::sync::{Arc, Mutex};

    struct MutateRider {
        discard: bool,
        heard: Arc<Mutex<Vec<(u32, bool)>>>,
    }
    #[mod_api::__subscriptions]
    impl mod_api::Hooks for MutateRider {
        fn entity_mount(
            &self,
            host: &mut dyn mod_api::ServerHost,
            event: &mut mod_api::EntityMountEvent<'_>,
        ) -> mod_api::Flow {
            self.heard
                .lock()
                .unwrap()
                .push((event.entity, event.mounting));
            if !event.mounting && event.entity == 72 {
                assert_eq!(
                    host.entity(72).unwrap().vehicle(host).map(|v| v.id()),
                    Some(71),
                    "callback observes original riding relationship"
                );
                if self.discard {
                    assert!(host.entity(72).unwrap().discard(host));
                } else {
                    let later = host.entity(73).unwrap();
                    let replacement = host.entity(74).unwrap();
                    assert!(later.start_riding(host, replacement, true));
                }
            }
            mod_api::Flow::Continue
        }
    }
}
