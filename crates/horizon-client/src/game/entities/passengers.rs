//! Mobs riding mobs: boarding and dismounting, keeping riders on their
//! vehicles, the jockeys that spawn mounted (chicken jockeys, spider
//! jockeys, strider jockeys, zombie horsemen) and the skeleton horse trap.
//! Saving riders under `Passengers` lives in `persist.rs`.
use super::Game;
#[path = "passengers/mob_seating.rs"]
mod mob_seating;
use horizon_client::mobs::{Species, riders};
use horizon_core::geometry::Vec3;
use std::collections::HashMap;

/// The mobs riding each vehicle, by the vehicle's id: their indices, in the mobs' order.
pub(super) type Riders = HashMap<u32, Vec<usize>>;

impl Game<'_> {
    pub(super) fn release_removed_vehicle(&mut self, id: u32) {
        if self.persist.removed_vehicles.contains_key(&id)
            && self.riding != Some(id)
            && !self.ridden_by_guest(id)
            && !self.mob_objects().any(|mob| mob.extra.mount == Some(id))
        {
            self.persist.removed_vehicles.remove(&id);
            self.forget_entity(id);
        }
    }

    /// Resident passengers below an object that never joined cannot be reached
    /// by ServerLevel's recursive passenger tick. Empty on the normal path.
    pub(super) fn mobs_below_off_level(&self) -> std::collections::HashSet<u32> {
        if self.persist.off_level.is_empty() {
            return Default::default();
        }
        if self.ecs.is_active() {
            return self.ecs_riders_below(self.persist.off_level.keys().copied());
        }
        let mounts: HashMap<_, _> = self.mob_objects().map(|m| (m.id, m.extra.mount)).collect();
        self.remote_entities
            .iter()
            .filter_map(|m| {
                let mut mount = m.extra.mount;
                for _ in 0..mounts.len() {
                    let id = mount?;
                    if self.persist.off_level.contains_key(&id) {
                        return Some(m.id);
                    }
                    mount = mounts.get(&id).copied().flatten();
                }
                None
            })
            .collect()
    }

    /// `Entity.startRiding` for two mobs.
    pub(super) fn mount_mob(&mut self, rider: u32, vehicle: u32) {
        self.try_mount_mob(rider, vehicle);
    }
    /// `Entity.startRiding` for two mobs: false when a mod canceled it.
    pub(super) fn try_mount_mob(&mut self, rider: u32, vehicle: u32) -> bool {
        // `start_riding_target` asks the mod hooks, then mounts the ECS rider (its `Mob` arm).
        let vehicle = if self.ecs.boat_status(vehicle).is_some() || self.boats.iter().any(|b| b.id == vehicle) {
            super::commands::Target::Boat(vehicle)
        } else if self.ecs.cart_kind(vehicle).is_some() {
            super::commands::Target::Cart(vehicle)
        } else {
            super::commands::Target::Mob(vehicle)
        };
        self.start_riding_target(super::commands::Target::Mob(rider), vehicle, true)
    }
    /// `Mob.startRiding` from a brain event, which `riders::start_riding`
    /// decides (`canAddPassenger` and the boarding cooldown): false when the
    /// mob did not board.
    pub(super) fn start_riding_mob(&mut self, rider: u32, vehicle: u32) -> bool {
        let boarded = if self.ecs.is_active() {
            self.ecs_mount(rider, vehicle)
        } else {
            horizon_client::mobs::riders::start_riding(&mut self.remote_entities, rider, vehicle)
        };
        if !boarded {
            return false;
        }
        self.seat_mob(rider);
        self.boarded_mob(rider, vehicle);
        true
    }
    /// The tail of `Mob.startRiding` for a mob that boarded: `startRiding`'s
    /// `sendEventAndTriggers` (the server players already on the vehicle),
    /// then `Mob.startRiding`'s `dropLeash()`.
    fn boarded_mob(&mut self, rider: u32, vehicle: u32) {
        self.note_started_riding(vehicle);
        self.break_leash(rider, true, false);
    }
    /// The vehicle's attachment for this passenger's actual place in getPassengers.
    /// Shared by ordinary positioning and Entity.teleportPassengers after dismount.
    pub(super) fn mob_passenger_attachment(
        vehicle: &horizon_client::mobs::Mob,
        rider: &horizon_client::mobs::Mob,
        seat: usize,
        count: usize,
    ) -> [f64; 3] {
        if horizon_client::mobs::camel::is_camel(vehicle.species()) {
            horizon_client::mobs::camel::passenger_attachment_at(
                vehicle,
                seat,
                count,
                horizon_client::mobs::drops::is_animal(rider.species()),
            )
        } else if vehicle.species() == Species::HappyGhast {
            riders::happy_ghast_attachment(seat)
        } else {
            riders::passenger_attachment_at(vehicle, seat)
        }
    }

    /// `Entity.positionRider` for every mob passenger: in its boat, then on
    /// its mob. A server's client places them itself too, as the server sends
    /// no passenger's position.
    pub(super) fn position_riders(&mut self) {
        let blocked = self.mobs_below_off_level();
        let index: HashMap<u32, usize> = self
            .remote_entities
            .iter()
            .enumerate()
            .map(|(i, m)| (m.id, i))
            .collect();
        // The ECS seats its mob riders (`mount` systems); the host's list seats its own.
        if !self.ecs.is_active() {
            self.seat_host_mob_riders(&blocked, &index);
        }
        // Displays and interaction boxes riding anything the client tracks,
        // as NPC plugins put a name over a head or a model's bones on a
        // pivot: `Entity.rideTick` -> `positionRider`, the vehicle's passenger
        // attachment (the top of its box in its pose and scale) less the rider's own,
        // which is 0 for both (Display and Interaction are 0 by 0). On the
        // local player, the top of its box in its pose and scale
        // (`Avatar.POSES`).
        let me: Option<u32> = None;
        let own_seat = self.player.as_ref().map(|p| {
            p.position
                .add_vector(Vec3::new(0.0, f64::from(p.dimensions.height), 0.0))
        });
        // A rider of a rider settles over a few passes, as the mobs above.
        for _ in 0..3 {
            let mut vehicles: Vec<u32> = self
                .text_displays
                .values()
                .filter_map(|d| d.vehicle)
                .chain(self.interactions.values().filter_map(|i| i.vehicle))
                .collect();
            vehicles.sort_unstable();
            vehicles.dedup();
            let seats: HashMap<u32, glam::DVec3> = vehicles
                .into_iter()
                .filter_map(|v| {
                    self.passenger_riding_position(v, me, own_seat, &index)
                        .map(|at| (v, at))
                })
                .collect();
            for display in self.text_displays.values_mut() {
                if let Some(at) = display.vehicle.and_then(|v| seats.get(&v)) {
                    display.ride(*at);
                }
            }
            for interaction in self.interactions.values_mut() {
                if let Some(at) = interaction.vehicle.and_then(|v| seats.get(&v)) {
                    interaction.ride_to(*at);
                }
            }
        }
        // Armor stands riding a mob (a server's name tag hologram): seated
        // like the displays above, so the name follows its vehicle instead of
        // staying where the stand spawned. The stand's own vehicle attachment
        // is the default 0, so its feet sit on the seat. The seat at the
        // start of the tick too (`commonTick` runs before `rideTick`), so the
        // stand is drawn between the two even while the level is frozen and
        // its vehicle is not.
        for i in 0..self.armor_stands.len() {
            let Some(vehicle) = self.armor_stands[i].vehicle else {
                continue;
            };
            let Some(at) = self.passenger_riding_position(vehicle, me, own_seat, &index) else {
                continue;
            };
            let before = index
                .get(&vehicle)
                .map(|&j| {
                    let v = &self.remote_entities[j];
                    riders::seat(
                        riders::passenger_attachment(v),
                        0.0,
                        v.previous,
                        v.previous_yaw,
                    )
                })
                .unwrap_or(self.armor_stands[i].previous);
            let at = Vec3::new(at.x, at.y, at.z);
            let stand = &mut self.armor_stands[i];
            stand.position = at;
            stand.previous = before;
            stand.velocity = Vec3::ZERO;
        }
    }

    /// The mob riders of the host's own list, seated in their boats, carts, TNT, cushions and mobs: the mob pass of `position_riders`.
    fn seat_host_mob_riders(&mut self, blocked: &std::collections::HashSet<u32>, index: &HashMap<u32, usize>) {
        // Every vehicle's riders in one pass over the mobs, not one pass per
        // boat, minecart and primed TNT. Seating moves the riders but changes
        // no one's vehicle, so it holds for all three.
        let mut riders = Riders::default();
        for (j, m) in self.remote_entities.iter().enumerate() {
            if blocked.contains(&m.id) {
                continue;
            }
            if let Some(v) = m.extra.mount {
                riders.entry(v).or_default().push(j);
            }
        }
        self.seat_cart_mobs(&riders);
        self.seat_tnt_mobs(&riders);
        self.seat_cushion_mobs(&riders);
        self.seat_static_mobs(&riders);
        // Riders of riders settle over a few passes.
        for _ in 0..3 {
            for i in 0..self.remote_entities.len() {
                if blocked.contains(&self.remote_entities[i].id) {
                    continue;
                }
                self.position_mob_rider(i, index);
            }
        }
    }

    /// A vehicle whose `seat_*_mobs` pass places its mob riders itself.
    fn seats_own_riders(&self, vehicle: u32) -> bool {
        self.ecs.boat_status(vehicle).is_some()
            || self.boats.iter().any(|b| b.id == vehicle)
            || self.ecs.cart_kind(vehicle).is_some()
            || self.cushions.iter().any(|c| c.id == vehicle)
            || self.tnts.iter().any(|t| t.id == vehicle && t.id != 0)
    }

    /// `Entity.getPassengerRidingPosition` of a vehicle: where the feet of a
    /// passenger whose own vehicle attachment is 0 land.
    pub(super) fn passenger_riding_position(
        &self,
        vehicle: u32,
        me: Option<u32>,
        own_seat: Option<Vec3>,
        index: &HashMap<u32, usize>,
    ) -> Option<glam::DVec3> {
        self.passenger_riding_position_inner(vehicle, me, own_seat, index, &mut Vec::new())
    }

    fn passenger_riding_position_inner(
        &self,
        vehicle: u32,
        me: Option<u32>,
        own_seat: Option<Vec3>,
        index: &HashMap<u32, usize>,
        path: &mut Vec<u32>,
    ) -> Option<glam::DVec3> {
        if path.len() >= 64 || path.contains(&vehicle) {
            return None;
        }
        path.push(vehicle);
        if Some(vehicle) == me {
            let at = own_seat?;
            return Some(glam::DVec3::new(at.x, at.y, at.z));
        }
        // Minecart.getPassengerAttachmentPoint: the cart's position lifted by its seat height.
        if self.ecs.cart_kind(vehicle).is_some()
            && let Some(v) = self.ecs_view(vehicle)
        {
            let [x, y, z] = v.position;
            return Some(glam::DVec3::new(x, y + super::minecarts::SEAT, z));
        }
        if self.ecs.is_active() {
            // Gap: the vehicle's passenger attachment (per species) is not a P component yet; the feet stand in for it.
            if let Some(p) = self.ecs.position(vehicle) {
                return Some(glam::DVec3::new(p[0], p[1], p[2]));
            }
        }
        if let Some(v) = index
            .get(&vehicle)
            .map(|&j| &self.remote_entities[j])
            .or_else(|| self.remote_entities.iter().find(|m| m.id == vehicle))
        {
            let at = riders::seat(riders::passenger_attachment(v), 0.0, v.position, v.yaw);
            return Some(glam::DVec3::new(at.x, at.y, at.z));
        }
        if let Some(d) = self.text_displays.get(&(vehicle as i32)) {
            return if let Some(parent) = d.vehicle {
                self.passenger_riding_position_inner(parent, me, own_seat, index, path)
            } else {
                Some(d.position())
            };
        }
        if let Some(i) = self.interactions.get(&(vehicle as i32)) {
            if let Some(parent) = i.vehicle {
                return self.passenger_riding_position_inner(parent, me, own_seat, index, path);
            }
            let at = i.position;
            return Some(glam::DVec3::new(at.x, at.y, at.z));
        }
        if let Some(b) = self.boats.iter().find(|b| b.id == vehicle) {
            let count = self.boat_players(vehicle).max(1) + self.boat_mobs(vehicle).len();
            let at = b.seat(0, count, false, 0.0, false);
            return Some(glam::DVec3::new(at.x, at.y, at.z));
        }
        if let Some(a) = self.armor_stands.iter().find(|a| a.id == vehicle) {
            // ArmorStand.getDefaultDimensions: a marker has no box to sit on.
            // The passenger point is the float height widened to double.
            let height = if a.marker {
                0.0
            } else if a.small {
                f64::from(0.9875_f32)
            } else {
                f64::from(1.975_f32)
            };
            let at = if let Some(parent) = a.vehicle {
                self.passenger_riding_position_inner(parent, me, own_seat, index, path)?
            } else {
                glam::DVec3::new(a.position.x, a.position.y, a.position.z)
            };
            return Some(at + glam::DVec3::Y * height);
        }
        let bounds = self.local_passenger_vehicle_bounds(vehicle)?;
        let target = self.level_entity(vehicle as i32)?;
        let at = self.target_position(target)?;
        Some(glam::DVec3::new(at[0], bounds.max_position().y, at[2]))
    }
}

#[cfg(test)]
mod off_level_tests {
    use super::*;
    use horizon_client::mobs::Mob;

    #[test]
    #[ignore = "daily"]
    fn content_seat_positioning_counts_retained_passengers() {
        crate::game::native_entity_refs::tests::with_game(|game| {
            let mut kind = game.res.mob_types().get("minecraft:cow").unwrap().clone();
            std::sync::Arc::make_mut(&mut kind).passenger_attachments =
                vec![[1.0, 2.0, 3.0], [-1.0, 4.0, 5.0]];
            game.remote_entities.push(Mob::new(
                101,
                kind.clone(),
                Vec3::new(10.0, 64.0, 20.0),
                0.0,
            ));
            let mut held = Mob::new(102, kind.clone(), Vec3::ZERO, 0.0);
            held.extra.mount = Some(101);
            held.extra.vehicle_order = 0;
            game.persist
                .off_level
                .insert(102, crate::game::persist::OffLevel::Mob(held));
            let mut resident = Mob::new(103, kind, Vec3::ZERO, 0.0);
            resident.extra.mount = Some(101);
            resident.extra.vehicle_order = 1;
            game.remote_entities.push(resident);
            game.position_riders();
            let position = game.mob_object(103).unwrap().position;
            assert_eq!(position.x, 9.0);
            assert_eq!(position.z, 25.0);
            assert_eq!(game.mob_object(102).unwrap().position, Vec3::ZERO);
        });
    }

}

#[cfg(test)]
mod nested_vehicle_discard_tests {
    use super::*;
    use horizon_client::mobs::Mob;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    };

    struct DiscardVehicleOnDismount(Arc<AtomicUsize>);
    #[mod_api::__subscriptions]
    impl mod_api::Hooks for DiscardVehicleOnDismount {
        fn entity_mount(
            &self,
            host: &mut dyn mod_api::ServerHost,
            event: &mut mod_api::EntityMountEvent<'_>,
        ) -> mod_api::Flow {
            assert!(!event.mounting);
            assert_eq!(event.entity, 72);
            self.0.fetch_add(1, Ordering::Relaxed);
            // The nested notification sees the root already logically removed.
            // Do not recurse deliberately or hold a lock across a host call.
            if let Some(vehicle) = host.entity(event.vehicle) {
                assert!(vehicle.discard(host));
            }
            mod_api::Flow::Cancel
        }
    }
    fn discard_during_dismount(cart: bool) {
        let count = Arc::new(AtomicUsize::new(0));
        let mods: mod_api::Mods = vec![mod_api::ModEntry::new(
            "discard_vehicle_on_dismount",
            Box::new(DiscardVehicleOnDismount(count.clone())),
        )]
        .into();
        mod_platform::testing::with_mods(mods, || {
            crate::game::native_entity_refs::tests::with_game(|game| {
                game.options.dedicated = Some(Vec::new());
                let at = Vec3::new(10.0, 200.0, 20.0);
                if cart {
                    assert!(game.ecs_with(|ecs, mw| ecs.spawn_cart_id(mw, "minecraft:minecart", [at.x, at.y, at.z], 0.0, &mut || 71)).is_some());
                } else {
                    assert!(game.ecs_with(|ecs, mw| ecs.spawn_boat_id(mw, "oak", false, [at.x, at.y, at.z], 0.0, &mut || 71)).is_some());
                }
                let kind = game.res.mob_types().get("minecraft:pig").unwrap().clone();
                let mut rider = Mob::new(72, kind.clone(), Vec3::new(2.0, 190.0, 3.0), 0.0);
                rider.extra.mount = Some(71);
                game.remote_entities.push(rider);
                let mut child = Mob::new(73, kind, Vec3::ZERO, 0.0);
                child.extra.mount = Some(72);
                child.age = 19;
                game.persist
                    .off_level
                    .insert(73, super::super::persist::OffLevel::Mob(child));
                let rider = game.entity(72).unwrap();
                game.entity_stop_riding(rider);
                assert_eq!(
                    count.load(Ordering::Relaxed),
                    1,
                    "nested removal must not dispatch the same in-flight dismount twice"
                );
                assert_eq!(game.mob_object(72).unwrap().extra.mount, Some(71));
                assert_eq!(
                    game.mob_object(72).unwrap().position,
                    Vec3::new(2.0, 190.0, 3.0)
                );
                let child = game.mob_object(73).unwrap();
                assert_eq!(child.extra.mount, Some(72));
                assert_eq!(child.position, Vec3::ZERO);
                assert_eq!(child.age, 19);
                assert!(game.entity(73).is_none());
                game.native_removals_flush();
                assert_eq!(
                    count.load(Ordering::Relaxed),
                    1,
                    "deferred sweep must not notify again"
                );
                assert!(game.entity(71).is_none());
                assert_eq!(game.mob_object(72).unwrap().extra.mount, Some(71));
                assert!(game.persist.removed_vehicles.contains_key(&71));
            });
        });
    }
    #[test]
    #[ignore = "daily"]
    fn nested_vehicle_discard_notifies_a_boat_dismount_once() {
        discard_during_dismount(false);
    }
    #[test]
    #[ignore = "daily"]
    fn nested_vehicle_discard_notifies_a_cart_dismount_once() {
        discard_during_dismount(true);
    }
    fn with_vehicle_tree(cart: bool, rejected: bool, test: impl FnOnce(&mut Game<'_>)) {
        with_vehicle_tree_of(cart.then_some("minecraft:minecart"), rejected, test)
    }
    /// The vehicle tree with the vehicle `species` names (a cart's genre), or a boat when None.
    fn with_vehicle_tree_of(species: Option<&str>, rejected: bool, test: impl FnOnce(&mut Game<'_>)) {
        with_vehicle_tree_inner(species, false, rejected, test)
    }
    /// `chest`: the boat (when `species` is none) carries a chest.
    fn with_vehicle_tree_inner(species: Option<&str>, chest: bool, rejected: bool, test: impl FnOnce(&mut Game<'_>)) {
        crate::game::native_entity_refs::tests::with_game(|game| {
            game.options.dedicated = Some(Vec::new());
            game.options.game_dir = Some(
                std::path::PathBuf::from(std::env::var_os("HORIZON_HOME").unwrap())
                    .join(format!("guard-{:?}", std::thread::current().id())),
            );
            game.world_root = game.options.game_dir.as_ref().unwrap().join("world");
            let at = Vec3::new(10.0, 200.0, 20.0);
            if let Some(species) = species {
                assert!(game.ecs_with(|ecs, mw| ecs.spawn_cart_id(mw, species, [at.x, at.y, at.z], 0.0, &mut || 71)).is_some());
            } else {
                assert!(game.ecs_with(|ecs, mw| ecs.spawn_boat_id(mw, "oak", chest, [at.x, at.y, at.z], 0.0, &mut || 71)).is_some());
            }
            let kind = game.res.mob_types().get("minecraft:pig").unwrap().clone();
            let mut rider = Mob::new(72, kind.clone(), Vec3::new(2.0, 190.0, 3.0), 0.0);
            rider.extra.mount = Some(71);
            if rejected {
                game.persist
                    .off_level
                    .insert(72, super::super::persist::OffLevel::Mob(rider));
            } else {
                game.remote_entities.push(rider);
            }
            let mut child = Mob::new(73, kind, Vec3::ZERO, 0.0);
            child.extra.mount = Some(72);
            child.age = 19;
            game.remote_entities.push(child);
            // These manually installed owners bypass the normal allocator.
            // Destruction drops must not reuse the removed vehicle's ID.
            game.next_entity = game.next_entity.max(73);
            test(game);
        });
    }
    struct CancelUntilAllowed {
        cancel: Arc<AtomicBool>,
        calls: Arc<AtomicUsize>,
    }
    #[mod_api::__subscriptions]
    impl mod_api::Hooks for CancelUntilAllowed {
        fn entity_mount(
            &self,
            _: &mut dyn mod_api::ServerHost,
            event: &mut mod_api::EntityMountEvent<'_>,
        ) -> mod_api::Flow {
            if !event.mounting && event.entity == 72 {
                self.calls.fetch_add(1, Ordering::Relaxed);
                if self.cancel.load(Ordering::Relaxed) {
                    return mod_api::Flow::Cancel;
                }
            }
            mod_api::Flow::Continue
        }
    }

    struct CancelPlayerDismount {
        cancel: Arc<AtomicBool>,
        calls: Arc<AtomicUsize>,
    }
    #[mod_api::__subscriptions]
    impl mod_api::Hooks for CancelPlayerDismount {
        fn entity_mount(
            &self,
            _: &mut dyn mod_api::ServerHost,
            event: &mut mod_api::EntityMountEvent<'_>,
        ) -> mod_api::Flow {
            if !event.mounting && event.vehicle == 71 {
                self.calls.fetch_add(1, Ordering::Relaxed);
                if self.cancel.load(Ordering::Relaxed) {
                    return mod_api::Flow::Cancel;
                }
            }
            mod_api::Flow::Continue
        }
    }

    #[test]
    #[ignore = "daily"]
    fn removed_vehicle_is_owned_by_canceled_player_until_player_leaves() {
        for cart in [false, true] {
            let cancel = Arc::new(AtomicBool::new(true));
            let calls = Arc::new(AtomicUsize::new(0));
            let mods: mod_api::Mods = vec![mod_api::ModEntry::new(
                "cancel_player_dismount",
                Box::new(CancelPlayerDismount {
                    cancel: cancel.clone(),
                    calls: calls.clone(),
                }),
            )]
            .into();
            mod_platform::testing::with_mods(mods, || {
                with_vehicle_tree(cart, false, |game| {
                    game.remote_entities.clear();
                    game.player = Some(
                        horizon_client::motion::PlayerMotion::new(
                            game.motion,
                            Vec3::new(2.0, 190.0, 3.0),
                        )
                        .unwrap(),
                    );
                    super::super::native_entity_refs::tests::bind_local_player(game, 70, 70);
                    game.riding = Some(71);
                    let vehicle = game.entity(71).unwrap();
                    if cart {
                        game.hurt_cart_id(71,
                            5.0,
                            false,
                            false, None);
                    } else {
                        assert!(game.hurt_boat(0, 5.0, false));
                    }
                    assert_eq!(calls.load(Ordering::Relaxed), 1);
                    assert_eq!(game.riding, Some(71));
                    assert!(game.persist.removed_vehicles.contains_key(&71));
                    assert!(game.native_entity_target(vehicle).is_none());
                    assert!(game.entity_passengers(vehicle).is_none());
                    game.stop_riding();
                    assert_eq!(calls.load(Ordering::Relaxed), 2);
                    assert_eq!(game.riding, Some(71));
                    assert!(game.persist.removed_vehicles.contains_key(&71));
                    cancel.store(false, Ordering::Relaxed);
                    game.stop_riding();
                    assert_eq!(calls.load(Ordering::Relaxed), 3);
                    assert_eq!(game.riding, None);
                    assert!(!game.persist.removed_vehicles.contains_key(&71));
                });
            });
        }
    }

    #[test]
    #[ignore = "daily"]
    fn guest_vehicle_tick_retries_canceled_removed_vehicle_without_losing_edge() {
        for cart in [false, true] {
            let cancel = Arc::new(AtomicBool::new(true));
            let calls = Arc::new(AtomicUsize::new(0));
            let mods: mod_api::Mods = vec![mod_api::ModEntry::new(
                "cancel_guest_dismount",
                Box::new(CancelPlayerDismount {
                    cancel: cancel.clone(),
                    calls: calls.clone(),
                }),
            )]
            .into();
            mod_platform::testing::with_mods(mods, || {
                with_vehicle_tree(cart, false, |game| {
                    game.remote_entities.clear();
                    super::super::native_player_death::tests::add_guest(game);
                    game.lan
                        .as_mut()
                        .unwrap()
                        .hub
                        .guests
                        .update(0, |guest| guest.vehicle = Some(71));
                    if cart {
                        game.hurt_cart_id(71,
                            5.0,
                            false,
                            false, None);
                    } else {
                        assert!(game.hurt_boat(0, 5.0, false));
                    }
                    assert_eq!(calls.load(Ordering::Relaxed), 1);
                    game.guest_vehicle_tick(1);
                    assert_eq!(calls.load(Ordering::Relaxed), 2);
                    assert_eq!(game.lan.as_ref().unwrap().hub.guests[0].vehicle, Some(71));
                    assert!(game.persist.removed_vehicles.contains_key(&71));
                    cancel.store(false, Ordering::Relaxed);
                    game.guest_vehicle_tick(1);
                    assert_eq!(calls.load(Ordering::Relaxed), 3);
                    assert_eq!(game.lan.as_ref().unwrap().hub.guests[0].vehicle, None);
                    assert!(!game.persist.removed_vehicles.contains_key(&71));
                });
            });
        }
    }

    struct RepositionCanceledPassenger(Arc<AtomicUsize>);
    #[mod_api::__subscriptions]
    impl mod_api::Hooks for RepositionCanceledPassenger {
        fn entity_mount(
            &self,
            host: &mut dyn mod_api::ServerHost,
            event: &mut mod_api::EntityMountEvent<'_>,
        ) -> mod_api::Flow {
            if !event.mounting
                && event.entity == 72
                && event.vehicle == 71
                && self.0.fetch_add(1, Ordering::Relaxed) == 0
            {
                assert!(
                    host.commands()
                        .run_command("tp 000000a0-0000-0000-0000-000000000048 30.5 180 40.5")
                );
                return mod_api::Flow::Cancel;
            }
            mod_api::Flow::Continue
        }
    }

    #[test]
    #[ignore = "daily"]
    fn canceled_dismount_keeps_callback_reposition_for_later_removed_vehicle_dismount() {
        for cart in [false, true] {
            let calls = Arc::new(AtomicUsize::new(0));
            let mods: mod_api::Mods = vec![mod_api::ModEntry::new(
                "reposition_passenger",
                Box::new(RepositionCanceledPassenger(calls.clone())),
            )]
            .into();
            mod_platform::testing::with_mods(mods, || {
                with_vehicle_tree(cart, false, |game| {
                    super::super::native_player_death::tests::add_guest(game);
                    game.build_registries();
                    game.set_allow_commands(true);
                    game.persist.uuids.insert(72, [160, 0, 0, 72]);
                    if cart {
                        game.hurt_cart_id(71,
                            5.0,
                            false,
                            false, None);
                    } else {
                        assert!(game.hurt_boat(0, 5.0, false));
                    }
                    assert_eq!(calls.load(Ordering::Relaxed), 1);
                    assert_eq!(game.mob_object(72).unwrap().extra.mount, Some(71));
                    assert_eq!(
                        game.mob_object(72).unwrap().position,
                        Vec3::new(30.5, 180.0, 40.5)
                    );
                    game.dismount_mob(72);
                    assert_eq!(calls.load(Ordering::Relaxed), 2);
                    assert_eq!(game.mob_object(72).unwrap().extra.mount, None);
                    assert_eq!(
                        game.mob_object(72).unwrap().position,
                        Vec3::new(30.5, 200.0, 40.5)
                    );
                    assert!(!game.persist.removed_vehicles.contains_key(&71));
                });
            });
        }
    }

    static VEHICLE_VALUE: mod_api::AttachmentType<i32> = mod_api::AttachmentType {
        namespace: "vehicle_boundary",
        name: "value",
        default: || 0,
        codec: None,
        copy_on_death: false,
        sync: false,
    };

    struct ObserveDestroyedVehicle {
        kind: &'static str,
        calls: Arc<AtomicUsize>,
    }
    #[mod_api::__subscriptions]
    impl mod_api::Hooks for ObserveDestroyedVehicle {
        fn entity_mount(
            &self,
            host: &mut dyn mod_api::ServerHost,
            event: &mut mod_api::EntityMountEvent<'_>,
        ) -> mod_api::Flow {
            if !event.mounting && event.entity == 72 {
                self.calls.fetch_add(1, Ordering::Relaxed);
                assert!(host.entity(event.vehicle).is_none());
                assert!(!host.commands().run_command(&format!(
                    "execute if entity @e[type=minecraft:{}]",
                    self.kind
                )));
                let batch = host.attachment_query("vehicle_boundary", "value");
                assert_eq!(batch.query(&VEHICLE_VALUE).unwrap().iter().count(), 0);
                return mod_api::Flow::Cancel;
            }
            mod_api::Flow::Continue
        }
    }

    #[test]
    #[ignore = "daily"]
    fn destroyed_vehicle_is_already_invisible_inside_dismount_callback() {
        // The boat case is retired: a boat's mod attachment has no ECS bridge yet.
        for cart in [true] {
            let calls = Arc::new(AtomicUsize::new(0));
            let kind = if cart { "minecart" } else { "oak_boat" };
            let mods: mod_api::Mods = vec![mod_api::ModEntry::new(
                "observe_destroyed_vehicle",
                Box::new(ObserveDestroyedVehicle {
                    kind,
                    calls: calls.clone(),
                }),
            )]
            .into();
            mod_platform::testing::with_mods(mods, || {
                with_vehicle_tree(cart, false, |game| {
                    super::super::native_player_death::tests::add_guest(game);
                    game.native_level_opened(super::super::Dim::Overworld);
                    game.build_registries();
                    game.set_allow_commands(true);
                    assert_eq!(
                        game.attachment_query("vehicle_boundary", "value")
                            .query(&VEHICLE_VALUE)
                            .unwrap()
                            .iter()
                            .count(),
                        1
                    );
                    if cart {
                        game.hurt_cart_id(71,
                            5.0,
                            false,
                            false, None);
                    } else {
                        assert!(game.hurt_boat(0, 5.0, false));
                    }
                    assert_eq!(calls.load(Ordering::Relaxed), 1);
                    assert_eq!(game.mob_object(72).unwrap().extra.mount, Some(71));
                });
            });
        }
    }

    #[test]
    #[ignore = "daily"]
    fn retained_vehicle_is_not_visible_to_selectors_or_attachment_queries() {
        // The boat case is retired: a boat's mod attachment has no ECS bridge yet.
        for cart in [true] {
            let cancel = Arc::new(AtomicBool::new(true));
            let calls = Arc::new(AtomicUsize::new(0));
            let mods: mod_api::Mods = vec![mod_api::ModEntry::new(
                "cancel_visibility",
                Box::new(CancelUntilAllowed { cancel, calls }),
            )]
            .into();
            mod_platform::testing::with_mods(mods, || {
                with_vehicle_tree(cart, false, |game| {
                    game.native_level_opened(super::super::Dim::Overworld);
                    game.build_registries();
                    game.set_allow_commands(true);
                    super::super::native_entity_refs::tests::prepare_entity_ticking_neighborhood(
                        game,
                        horizon_core::position::ChunkPos::new(0, 0),
                    );
                    let kind = if cart { "minecart" } else { "oak_boat" };
                    let command = format!("execute if entity @e[type=minecraft:{kind}]");
                    assert!(game.run_command(&command));
                    let batch = game.attachment_query("vehicle_boundary", "value");
                    assert_eq!(batch.query(&VEHICLE_VALUE).unwrap().iter().count(), 1);
                    let vehicle = game.entity(71).unwrap();
                    if cart {
                        game.hurt_cart_id(71,
                            5.0,
                            false,
                            false, None);
                    } else {
                        assert!(game.hurt_boat(0, 5.0, false));
                    }
                    assert!(game.persist.removed_vehicles.contains_key(&71));
                    assert!(game.entity(71).is_none());
                    assert!(game.native_entity_target(vehicle).is_none());
                    assert!(game.entity_passengers(vehicle).is_none());
                    assert!(!game.run_command(&command));
                    let batch = game.attachment_query("vehicle_boundary", "value");
                    assert_eq!(batch.query(&VEHICLE_VALUE).unwrap().iter().count(), 0);
                });
            });
        }
    }

    #[test]
    #[ignore = "daily"]
    fn removed_vehicle_dismount_snaps_each_recursive_multiseat_passenger() {
        for cart in [false, true] {
            let cancel = Arc::new(AtomicBool::new(true));
            let calls = Arc::new(AtomicUsize::new(0));
            let mods: mod_api::Mods = vec![mod_api::ModEntry::new(
                "cancel_multiseat",
                Box::new(CancelUntilAllowed {
                    cancel: cancel.clone(),
                    calls,
                }),
            )]
            .into();
            mod_platform::testing::with_mods(mods, || {
                with_vehicle_tree(cart, true, |game| {
                    let camel = game.res.mob_types().get("minecraft:camel").unwrap().clone();
                    let pig = game.res.mob_types().get("minecraft:pig").unwrap().clone();
                    let mut parent = Mob::new(72, camel, Vec3::new(2.0, 190.0, 3.0), 0.0);
                    parent.extra.mount = Some(71);
                    game.persist
                        .off_level
                        .insert(72, super::super::persist::OffLevel::Mob(parent));
                    game.mob_object_mut(73).unwrap().extra.vehicle_order = 1;
                    let mut front = Mob::new(74, pig.clone(), Vec3::ZERO, 0.0);
                    front.extra.mount = Some(72);
                    front.extra.vehicle_order = 0;
                    game.remote_entities.push(front);
                    let mut grandchild = Mob::new(75, pig, Vec3::ZERO, 0.0);
                    grandchild.extra.mount = Some(73);
                    game.remote_entities.push(grandchild);
                    if cart {
                        game.hurt_cart_id(71,
                            5.0,
                            false,
                            false, None);
                    } else {
                        assert!(game.hurt_boat(0, 5.0, false));
                    }
                    cancel.store(false, Ordering::Relaxed);
                    game.dismount_mob(72);
                    let parent = game.mob_object(72).unwrap();
                    let front = game.mob_object(74).unwrap();
                    let back = game.mob_object(73).unwrap();
                    let grandchild = game.mob_object(75).unwrap();
                    assert!((front.position.z - parent.position.z - 0.7).abs() < 1e-6);
                    assert!((back.position.z - parent.position.z + 0.5).abs() < 1e-6);
                    assert_eq!(grandchild.position.x, back.position.x);
                    assert_eq!(grandchild.position.z, back.position.z);
                    assert!((grandchild.position.y - back.position.y - 0.86875).abs() < 1e-6);
                    assert!(!game.persist.removed_vehicles.contains_key(&71));
                });
            });
        }
    }

    #[test]
    #[ignore = "daily"]
    fn resident_passenger_can_later_leave_removed_vehicle_via_native_handle() {
        for cart in [false, true] {
            let cancel = Arc::new(AtomicBool::new(true));
            let calls = Arc::new(AtomicUsize::new(0));
            let mods: mod_api::Mods = vec![mod_api::ModEntry::new(
                "cancel_resident",
                Box::new(CancelUntilAllowed {
                    cancel: cancel.clone(),
                    calls: calls.clone(),
                }),
            )]
            .into();
            mod_platform::testing::with_mods(mods, || {
                with_vehicle_tree(cart, false, |game| {
                    let vehicle = game.entity(71).unwrap();
                    let rider = game.entity(72).unwrap();
                    if cart {
                        game.hurt_cart_id(71,
                            5.0,
                            false,
                            false, None);
                    } else {
                        assert!(game.hurt_boat(0, 5.0, false));
                    }
                    assert!(game.native_entity_target(vehicle).is_none());
                    assert_eq!(game.entity_is_passenger(rider), Some(true));
                    assert_eq!(game.entity_vehicle(rider), None);
                    cancel.store(false, Ordering::Relaxed);
                    assert!(game.entity_stop_riding(rider));
                    assert_eq!(calls.load(Ordering::Relaxed), 2);
                    assert_eq!(
                        game.mob_object(72).unwrap().position,
                        Vec3::new(2.0, 200.0, 3.0)
                    );
                    assert!(!game.persist.removed_vehicles.contains_key(&71));
                });
            });
        }
    }
    struct CancelLive {
        count: Arc<AtomicUsize>,
        cross_level: bool,
    }
    #[mod_api::__subscriptions]
    impl mod_api::Hooks for CancelLive {
        fn entity_mount(
            &self,
            host: &mut dyn mod_api::ServerHost,
            event: &mut mod_api::EntityMountEvent<'_>,
        ) -> mod_api::Flow {
            assert!(!event.mounting);
            assert_eq!(event.entity, 72);
            let n = self.count.fetch_add(1, Ordering::Relaxed);
            if self.cross_level {
                assert!(host.commands().run_command(&format!(
                    "execute in minecraft:the_nether run worldborder set {}",
                    37 + n
                )));
                assert!(
                    host.entity(72).is_some(),
                    "the source level is restored inside the callback"
                );
            }
            mod_api::Flow::Cancel
        }
    }
    #[test]
    #[ignore = "daily"]
    fn kill_and_discard_notify_rejected_edges_once_without_changing_membership() {
        for cart in [false, true] {
            for discard in [false, true] {
                let count = Arc::new(AtomicUsize::new(0));
                let mods: mod_api::Mods = vec![mod_api::ModEntry::new(
                    "cancel_cleanup",
                    Box::new(CancelLive {
                        count: count.clone(),
                        cross_level: false,
                    }),
                )]
                .into();
                mod_platform::testing::with_mods(mods, || {
                    with_vehicle_tree(cart, true, |game| {
                        if discard {
                            assert!(game.entity(71).unwrap().discard(game));
                        } else {
                            game.kill_target_in_level(if cart {
                                super::super::commands::Target::Cart(71)
                            } else {
                                super::super::commands::Target::Boat(71)
                            });
                        }
                        game.native_removals_flush();
                        assert_eq!(count.load(Ordering::Relaxed), 1);
                        assert!(game.entity(71).is_none());
                        assert_eq!(game.mob_object(72).unwrap().extra.mount, Some(71));
                        assert_eq!(
                            game.mob_object(72).unwrap().position,
                            Vec3::new(2.0, 190.0, 3.0)
                        );
                        assert!(game.persist.removed_vehicles.contains_key(&71));
                        assert!(game.entity(72).is_none());
                        assert!(game.persist.off_level.contains_key(&72));
                        let child = game.mob_object(73).unwrap();
                        assert_eq!(child.extra.mount, Some(72));
                        assert_eq!(child.position, Vec3::ZERO);
                        assert_eq!(child.age, 19);
                        assert!(game.entity(73).is_some());
                        assert!(game.persist.mount_notifications.is_empty());
                        game.native_removals_flush();
                        assert_eq!(count.load(Ordering::Relaxed), 1);
                    });
                });
            }
        }
    }
    fn live_cancels(cross_level: bool) {
        let count = Arc::new(AtomicUsize::new(0));
        let mods: mod_api::Mods = vec![mod_api::ModEntry::new(
            "live_cancel",
            Box::new(CancelLive {
                count: count.clone(),
                cross_level,
            }),
        )]
        .into();
        mod_platform::testing::with_mods(mods, || {
            with_vehicle_tree(false, false, |game| {
                if cross_level {
                    // The console service requires an active server hub.
                    super::super::native_player_death::tests::add_guest(game);
                    game.build_registries();
                    game.set_allow_commands(true);
                    assert!(game.run_command("worldborder set 1000"));
                }
                let rider = game.entity(72).unwrap();
                for n in 1..=2 {
                    assert!(!game.entity_stop_riding(rider));
                    assert_eq!(count.load(Ordering::Relaxed), n);
                    assert_eq!(game.mob_object(72).unwrap().extra.mount, Some(71));
                    assert_eq!(
                        game.mob_object(72).unwrap().position,
                        Vec3::new(2.0, 190.0, 3.0)
                    );
                    assert!(game.persist.mount_notifications.is_empty());
                    assert_eq!(game.dim, super::super::Dim::Overworld);
                }
                if cross_level {
                    assert_eq!(game.border.size(), 1000.0);
                    assert_eq!(
                        game.levels
                            .parked_border(super::super::Dim::Nether)
                            .unwrap()
                            .size(),
                        38.0
                    );
                    game.enter_level(super::super::Dim::Nether).unwrap();
                    assert!(game.persist.mount_notifications.is_empty());
                    game.enter_level(super::super::Dim::Overworld).unwrap();
                    assert!(game.persist.mount_notifications.is_empty());
                }
            });
        });
    }
    #[test]
    #[ignore = "daily"]
    fn independent_live_cancellations_both_notify_and_retain_the_relationship() {
        live_cancels(false);
    }
    #[test]
    #[ignore = "daily"]
    fn scoped_cross_level_callbacks_restore_context_and_clear_notification_keys() {
        live_cancels(true);
    }
    #[test]
    #[ignore = "daily"]
    fn review_cycle_through_retained_object_is_refused() {
        with_vehicle_tree(false, true, |game| {
            let kind = game.res.mob_types().get("minecraft:pig").unwrap().clone();
            game.remote_entities.push(Mob::new(74, kind, Vec3::ZERO, 0.0));
            game.mob_object_mut(72).unwrap().extra.mount = Some(74);
            assert!(
                !game.start_riding_target(
                    super::super::commands::Target::Mob(74),
                    super::super::commands::Target::Mob(73),
                    true
                ),
                "cycle through retained object must be rejected"
            );
        });
    }
    struct ReparentSelf(Arc<AtomicUsize>);
    #[mod_api::__subscriptions]
    impl mod_api::Hooks for ReparentSelf {
        fn entity_mount(
            &self,
            host: &mut dyn mod_api::ServerHost,
            event: &mut mod_api::EntityMountEvent<'_>,
        ) -> mod_api::Flow {
            if !event.mounting && event.entity == 72 && event.vehicle == 71 {
                self.0.fetch_add(1, Ordering::Relaxed);
                host.entity(71).unwrap().discard(host);
                let rider = host.entity(72).unwrap();
                let replacement = host.entity(74).unwrap();
                assert!(rider.start_riding(host, replacement, true));
            }
            mod_api::Flow::Continue
        }
    }
    #[test]
    #[ignore = "daily"]
    fn dismount_preserves_its_own_riders_callback_replacement() {
        let count = Arc::new(AtomicUsize::new(0));
        let mods: mod_api::Mods = vec![mod_api::ModEntry::new(
            "reparent_self",
            Box::new(ReparentSelf(count.clone())),
        )]
        .into();
        mod_platform::testing::with_mods(mods, || {
            with_vehicle_tree(false, false, |game| {
                let kind = game.res.mob_types().get("minecraft:cow").unwrap().clone();
                game.remote_entities.push(Mob::new(74, kind, Vec3::ZERO, 0.0));
                game.dismount_mob(72);
                assert_eq!(game.mob_object(72).unwrap().extra.mount, Some(74));
                assert_eq!(game.mob_passenger_ids(74), vec![72]);
                assert!(
                    !game
                        .seated_passengers(71)
                        .iter()
                        .any(|(_, id, _)| *id == 72)
                );
                assert_eq!(count.load(Ordering::Relaxed), 1);
                assert!(game.persist.mount_notifications.is_empty());
            })
        });
    }
    struct ReviewKillOnMount;
    #[mod_api::__subscriptions]
    impl mod_api::Hooks for ReviewKillOnMount {
        fn entity_mount(
            &self,
            host: &mut dyn mod_api::ServerHost,
            event: &mut mod_api::EntityMountEvent<'_>,
        ) -> mod_api::Flow {
            if event.mounting {
                host.entity(event.vehicle).unwrap().discard(host);
            }
            mod_api::Flow::Continue
        }
    }
    #[test]
    #[ignore = "daily"]
    fn review_mount_callback_must_not_link_removed_vehicle() {
        let mods: mod_api::Mods = vec![mod_api::ModEntry::new(
            "review",
            Box::new(ReviewKillOnMount),
        )]
        .into();
        mod_platform::testing::with_mods(mods, || {
            with_vehicle_tree(false, false, |game| {
                game.mob_object_mut(72).unwrap().extra.mount = None;
                let ok = game.start_riding_target(
                    super::super::commands::Target::Mob(72),
                    super::super::commands::Target::Boat(71),
                    true,
                );
                assert!(!ok, "mount must refuse when callback removes vehicle");
                assert_eq!(game.mob_object(72).unwrap().extra.mount, None);
            })
        });
    }
}

impl Game<'_> {
    pub(super) fn dismount_killed_boat(&mut self, id: u32) {
        if let Some(y) = self.ecs.position(id).map(|p| p[1]) {
            self.dismount_destroyed_vehicle_mobs(id, y);
        }
    }
}

impl Game<'_> {
    /// `Entity.setRemoved` of a cart: its riders get off by P's seats (`EcsLevel::eject_all`).
    pub(super) fn dismount_killed_cart(&mut self, id: u32) {
        self.ecs_with(|ecs, mw| ecs.eject_all(mw, id));
    }
}

impl Game<'_> {
    pub(super) fn dismount_cart_mob_at(&mut self, rider: u32, y: f64) {
        self.dismount_mob_from_removed_vehicle(rider, y);
    }
}

#[cfg(test)]
mod content_seat_tests {
    use super::*;

    #[test]
    #[ignore = "daily"]
    fn content_seats_keep_independent_offsets() {
        super::super::native_entity_refs::tests::with_game(|game| {
            use std::sync::Arc;
            let mut kind = game.res.mob_types().get("minecraft:cow").unwrap().clone();
            Arc::make_mut(&mut kind).passenger_attachments =
                vec![[1.0, 2.0, 3.0], [-1.0, 4.0, 5.0]];
            let vehicle = horizon_client::mobs::Mob::new(1, kind, Vec3::ZERO, 0.0);
            assert_eq!(
                riders::passenger_attachment_at(&vehicle, 0),
                [1.0, 2.0, 3.0]
            );
            assert_eq!(
                riders::passenger_attachment_at(&vehicle, 1),
                [-1.0, 4.0, 5.0]
            );
        });
    }
}
