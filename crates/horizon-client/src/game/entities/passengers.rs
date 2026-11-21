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
    /// Keep the actual removed object only while a passenger still refers to it.
    pub(super) fn retain_removed_vehicle(
        &mut self,
        id: u32,
        vehicle: super::persist::RemovedVehicle,
    ) {
        if self.riding == Some(id)
            || self.ridden_by_guest(id)
            || self.mob_objects().any(|mob| mob.extra.mount == Some(id))
        {
            vehicle.retire_attachment_registration();
            self.persist.removed_vehicles.insert(id, vehicle);
            self.score_destroyed_entities();
        }
    }

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
    /// Riders sit on their vehicles (`Entity.positionRider`), mobs or boats,
    /// and let go when the vehicle dies or is gone.
    pub(super) fn sync_riders(&mut self) {
        if !self.persist.off_level.is_empty() || !self.persist.removed_vehicles.is_empty() {
            self.sync_retained_riders();
            self.position_riders();
            {
                self.position_saved_passengers();
            }
            if !self.persist.removed_vehicles.is_empty()
                || self
                    .persist
                    .off_level
                    .values()
                    .any(crate::game::persist::OffLevel::is_removed)
            {
                // Keep a removed vehicle only while cancellation leaves actual
                // references to it; completed dismounts release its ownership.
                self.prune_entity_memory();
            }
            return;
        }
        if self.ecs.is_active() {
            // The ECS seats its riders; no sweep over the mobs runs here.
            self.position_riders();
            {
                self.position_saved_passengers();
            }
            return;
        }
        // Entity.baseTick.
        for m in &mut self.remote_entities {
            if m.extra.boarding_cooldown > 0 {
                m.extra.boarding_cooldown -= 1;
            }
        }
        let boats: std::collections::HashSet<u32> = self.boats.iter().map(|b| b.id).collect();
        let cushions: std::collections::HashSet<u32> = self.cushions.iter().map(|c| c.id).collect();
        let tnts: std::collections::HashSet<u32> = self
            .tnts
            .iter()
            .map(|t| t.id)
            .filter(|&id| id != 0)
            .collect();
        let index: HashMap<u32, usize> = self
            .remote_entities
            .iter()
            .enumerate()
            .map(|(i, m)| (m.id, i))
            .collect();
        let mut riding: HashMap<u32, (i64, u32)> = HashMap::new();
        for i in 0..self.remote_entities.len() {
            let Some(v) = self.remote_entities[i].extra.mount else {
                continue;
            };
            // Not a text display: a server's display mounts come through
            // `remote_set_passengers`, which seats them in `position_riders`.
            let valid = !self.remote_entities[i].removed
                && (boats.contains(&v)
                    || self.ecs.cart_kind(v).is_some()
                    || tnts.contains(&v)
                    || cushions.contains(&v)
                    || self.seat(v).is_some()
                    || self.passenger_present(v)
                    || index.get(&v).is_some_and(|&j| !self.remote_entities[j].removed));
            if valid {
                let place = (self.remote_entities[i].extra.vehicle_order, self.remote_entities[i].id);
                riding
                    .entry(v)
                    .and_modify(|first| *first = (*first).min(place))
                    .or_insert(place);
            } else {
                let m = &mut self.remote_entities[i];
                m.extra.mount = None;
                m.extra.passenger = false;
            }
        }
        for m in &mut self.remote_entities {
            m.extra.rider = riding.get(&m.id).map(|&(_, id)| id);
        }
        self.position_riders();
        {
            self.position_saved_passengers();
        }
    }
    fn sync_retained_riders(&mut self) {
        if self.ecs.is_active() {
            // Gap: the invalid retained edges and the boarding cooldown have no ECS component yet (report).
            return;
        }
        let invalid: Vec<_> = self
            .mob_objects()
            .filter_map(|m| {
                let v = m.extra.mount?;
                // Removed vehicles are retried only in the passenger's own
                // eligible entity tick, after its cancellable Pre event.
                if self.persist.removed_vehicles.contains_key(&v) {
                    return None;
                }
                let valid = !m.removed
                    && (self.mob_object(v).is_some_and(|m| !m.removed)
                        || self.is_boat(v)
                        || self.ecs.cart_kind(v).is_some()
                        || self.cushions.iter().any(|c| c.id == v)
                        || self.tnts.iter().any(|t| t.id == v && v != 0)
                        || self.seat(v).is_some()
                        || self.passenger_present(v));
                (!valid).then_some((m.id, v))
            })
            .collect();
        for (id, vehicle) in invalid {
            if self.mob_object(id).and_then(|m| m.extra.mount) != Some(vehicle) {
                continue;
            }
            // Dismount supplies positioning, descendant snapping and the
            // cancellable mount hook. Never overwrite its post-callback edge.
            self.dismount_mob(id);
        }
        let mut first: HashMap<u32, (i64, u32)> = HashMap::new();
        for m in self.mob_objects() {
            if let Some(v) = m.extra.mount {
                let place = (m.extra.vehicle_order, m.id);
                first
                    .entry(v)
                    .and_modify(|p| *p = (*p).min(place))
                    .or_insert(place);
            }
        }
        let ids: Vec<_> = self.mob_objects().map(|m| m.id).collect();
        for id in ids {
            self.mob_object_mut(id).unwrap().extra.rider = first.get(&id).map(|&(_, id)| id);
        }
        let blocked = self.mobs_below_off_level();
        for m in &mut self.remote_entities {
            if !blocked.contains(&m.id) && m.extra.boarding_cooldown > 0 {
                m.extra.boarding_cooldown -= 1;
            }
        }
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
    fn dying_riding_edges_do_not_depend_on_unrelated_retained_objects() {
        for retained in [false, true] {
            for dying_vehicle in [false, true] {
                crate::game::native_entity_refs::tests::with_game(|game| {
                    let kind = game.res.mob_types().get("minecraft:cow").unwrap().clone();
                    let mut vehicle = Mob::new(161, kind.clone(), Vec3::ZERO, 0.0);
                    let mut rider = Mob::new(162, kind.clone(), Vec3::ZERO, 0.0);
                    rider.extra.mount = Some(161);
                    if dying_vehicle {
                        vehicle.health = 0.0;
                    } else {
                        rider.health = 0.0;
                    }
                    game.remote_entities.push(vehicle);
                    game.remote_entities.push(rider);
                    if retained {
                        game.persist.off_level.insert(
                            169,
                            crate::game::persist::OffLevel::Mob(Mob::new(
                                169,
                                kind,
                                Vec3::ZERO,
                                0.0,
                            )),
                        );
                    }
                    game.sync_riders();
                    assert_eq!(
                        game.mob_object(162).unwrap().extra.mount,
                        Some(161),
                        "retained={retained}, dying_vehicle={dying_vehicle}"
                    );
                });
            }
        }
    }

    #[test]
    #[ignore = "daily"]
    fn first_rider_order_does_not_depend_on_unrelated_retained_objects() {
        for retained in [false, true] {
            crate::game::native_entity_refs::tests::with_game(|game| {
                let kind = game.res.mob_types().get("minecraft:cow").unwrap().clone();
                let mut vehicle = Mob::new(171, kind.clone(), Vec3::ZERO, 0.0);
                vehicle.extra.rider = Some(172);
                game.remote_entities.push(vehicle);
                for (id, order) in [(172, 1), (173, 0)] {
                    let mut rider = Mob::new(id, kind.clone(), Vec3::ZERO, 0.0);
                    rider.extra.mount = Some(171);
                    rider.extra.vehicle_order = order;
                    game.remote_entities.push(rider);
                }
                if retained {
                    game.persist.off_level.insert(
                        179,
                        crate::game::persist::OffLevel::Mob(Mob::new(179, kind, Vec3::ZERO, 0.0)),
                    );
                }
                game.sync_riders();
                assert_eq!(
                    game.mob_object(171).unwrap().extra.rider,
                    Some(173),
                    "retained={retained}"
                );
            });
        }
    }

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

    #[test]
    #[ignore = "daily"]
    fn dismounted_retained_multiseat_vehicle_snaps_each_frozen_passenger() {
        for name in ["minecraft:camel", "minecraft:happy_ghast"] {
            crate::game::native_entity_refs::tests::with_game(|game| {
                let pig = game.res.mob_types().get("minecraft:pig").unwrap().clone();
                let kind = game.res.mob_types().get(name).unwrap().clone();
                let mut root = Mob::new(70, pig.clone(), Vec3::new(10.0, 200.0, 20.0), 0.0);
                root.removed = true;
                let mut vehicle = Mob::new(71, kind, root.position, 0.0);
                vehicle.extra.mount = Some(70);
                game.remote_entities.push(root);
                game.persist
                    .off_level
                    .insert(71, crate::game::persist::OffLevel::Mob(vehicle));
                // Camel.getPassengerAttachmentPoint: two Animals occupy z +0.7/-0.5.
                // EntityTypes.HAPPY_GHAST: front, left, back, right, all at y +4.
                let expected: &[(f64, f64)] = if name == "minecraft:camel" {
                    &[(0.0, 0.7), (0.0, -0.5)]
                } else {
                    &[(0.0, 1.7), (-1.7, 0.0), (0.0, -1.7), (1.7, 0.0)]
                };
                // Reverse storage order: seats follow vehicle_order, not resident order.
                for seat in (0..expected.len()).rev() {
                    let id = 72 + seat as u32;
                    let mut rider = Mob::new(id, pig.clone(), Vec3::ZERO, 0.0);
                    rider.age = 19;
                    rider.extra.mount = Some(71);
                    rider.extra.vehicle_order = seat as i64;
                    if name == "minecraft:happy_ghast" && seat == 2 {
                        game.persist
                            .off_level
                            .insert(id, crate::game::persist::OffLevel::Mob(rider));
                        let mut child = Mob::new(80, pig.clone(), Vec3::ZERO, 0.0);
                        child.extra.mount = Some(id);
                        child.age = 23;
                        game.remote_entities.push(child);
                    } else {
                        game.remote_entities.push(rider);
                    }
                }
                game.sync_riders();
                // The normal pass must not overwrite the one-time snap of frozen descendants.
                game.position_riders();
                let vehicle = game.mob_object(71).unwrap();
                assert_eq!(vehicle.extra.mount, None);
                for (seat, &(x, z)) in expected.iter().enumerate() {
                    let rider = game.mob_object(72 + seat as u32).unwrap();
                    assert!((rider.position.x - vehicle.position.x - x).abs() < 1e-6);
                    assert!((rider.position.z - vehicle.position.z - z).abs() < 1e-6);
                    assert_eq!(rider.previous, rider.position);
                    assert_eq!(rider.age, 19);
                    if name == "minecraft:happy_ghast" {
                        assert!((rider.position.y - vehicle.position.y - 4.0).abs() < 1e-6);
                    }
                }
                if name == "minecraft:happy_ghast" {
                    let parent = game.mob_object(74).unwrap();
                    let child = game.mob_object(80).unwrap();
                    assert_eq!(child.position.x, parent.position.x);
                    assert_eq!(child.position.z, parent.position.z);
                    assert!((child.position.y - parent.position.y - 0.86875).abs() < 1e-6);
                    assert_eq!(child.age, 23);
                }
            });
        }
    }

    struct CountCanceledDismount(std::sync::Arc<std::sync::atomic::AtomicUsize>);
    #[mod_api::__subscriptions]
    impl mod_api::Hooks for CountCanceledDismount {
        fn entity_mount(
            &self,
            _: &mut dyn mod_api::ServerHost,
            _: &mut mod_api::EntityMountEvent<'_>,
        ) -> mod_api::Flow {
            self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            mod_api::Flow::Cancel
        }
    }

    #[test]
    #[ignore = "daily"]
    fn destroyed_resident_vehicles_keep_canceled_rejected_edges() {
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };
        for cart in [false, true] {
            let cancel = Arc::new(AtomicUsize::new(0));
            let mods: mod_api::Mods = vec![mod_api::ModEntry::new(
                "toggle_dismount",
                Box::new(CountCanceledDismount(cancel.clone())),
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
                    let pig = game.res.mob_types().get("minecraft:pig").unwrap().clone();
                    let mut rider = Mob::new(72, pig.clone(), Vec3::new(5.0, 190.0, 6.0), 0.0);
                    rider.extra.mount = Some(71);
                    game.persist
                        .off_level
                        .insert(72, super::super::persist::OffLevel::Mob(rider));
                    let mut child = Mob::new(73, pig, Vec3::ZERO, 0.0);
                    child.extra.mount = Some(72);
                    child.age = 19;
                    game.remote_entities.push(child);
                    if cart {
                        game.hurt_cart_id(71,
                            5.0,
                            true,
                            false, None);
                    } else {
                        assert!(game.hurt_boat(0, 5.0, true));
                    }
                    assert_eq!(cancel.load(Ordering::Relaxed), 1);
                    game.sync_riders();
                    let rider = game.mob_object(72).unwrap();
                    assert_eq!(rider.extra.mount, Some(71));
                    assert_eq!(rider.position, Vec3::new(5.0, 190.0, 6.0));
                    let child = game.mob_object(73).unwrap();
                    assert_eq!(child.position, Vec3::ZERO);
                    assert_eq!(child.age, 19);
                    game.prune_entity_memory();
                    assert!(game.persist.removed_vehicles.contains_key(&71));
                    assert!(game.persist.off_level.contains_key(&72));
                    assert!(game.remote_entities.iter().any(|m| m.id == 73));
                    assert!(!game.remote_entities.iter().any(|m| m.id == 72));
                    assert_eq!(cancel.load(Ordering::Relaxed), 1);
                });
            });
        }
    }

    struct CancelDismount;
    #[mod_api::__subscriptions]
    impl mod_api::Hooks for CancelDismount {
        fn entity_mount(
            &self,
            _: &mut dyn mod_api::ServerHost,
            event: &mut mod_api::EntityMountEvent<'_>,
        ) -> mod_api::Flow {
            assert!(!event.mounting);
            mod_api::Flow::Cancel
        }
    }

    #[test]
    #[ignore = "daily"]
    fn invalid_retained_mount_uses_cancellable_dismount() {
        for cancel in [false, true] {
            let mods: mod_api::Mods = if cancel {
                vec![mod_api::ModEntry::new(
                    "cancel_dismount",
                    Box::new(CancelDismount),
                )]
                .into()
            } else {
                Vec::new().into()
            };
            mod_platform::testing::with_mods(mods, || {
                crate::game::native_entity_refs::tests::with_game(|game| {
                    game.options.dedicated = Some(Vec::new());
                    let pig = game.res.mob_types().get("minecraft:pig").unwrap().clone();
                    let mut root = Mob::new(71, pig.clone(), Vec3::new(1.0, 200.0, 0.5), 0.0);
                    root.removed = true;
                    root.extra.rider = Some(72);
                    let mut rider = Mob::new(72, pig.clone(), Vec3::new(1.0, 190.0, 0.5), 0.0);
                    rider.extra.mount = Some(71);
                    rider.extra.passenger = true;
                    let mut child = Mob::new(73, pig, rider.position, 0.0);
                    child.extra.mount = Some(72);
                    child.age = 19;
                    game.remote_entities.push(child);
                    game.remote_entities.push(root);
                    game.persist
                        .off_level
                        .insert(72, crate::game::persist::OffLevel::Mob(rider));
                    game.sync_riders();
                    let rider = game.mob_object(72).unwrap();
                    if cancel {
                        assert_eq!(rider.extra.mount, Some(71));
                        assert!(rider.extra.passenger);
                        assert_eq!(rider.position.y, 190.0);
                        assert_eq!(game.mob_object(71).unwrap().extra.rider, Some(72));
                        assert_eq!(game.mob_object(73).unwrap().position.y, 190.0);
                    } else {
                        assert_eq!(rider.extra.mount, None);
                        assert!(!rider.extra.passenger);
                        assert_eq!(
                            rider.extra.boarding_cooldown,
                            super::super::boat_passengers::BOARDING_COOLDOWN
                        );
                        assert_eq!(rider.position.y, 200.0);
                        assert_eq!(game.mob_object(71).unwrap().extra.rider, None);
                        assert!(game.mob_object(73).unwrap().position.y > 200.0);
                    }
                    let child = game.mob_object(73).unwrap();
                    assert_eq!(child.age, 19);
                    assert_eq!(child.extra.mount, Some(72));
                });
            });
        }
    }

    #[test]
    #[ignore = "daily"]
    fn dying_objects_keep_passenger_links_until_removal() {
        crate::game::native_entity_refs::tests::with_game(|game| {
            let pig = game.res.mob_types().get("minecraft:pig").unwrap().clone();
            let mut root = Mob::new(71, pig.clone(), Vec3::new(1.0, 200.0, 0.5), 0.0);
            root.health = 0.0;
            let mut parent = Mob::new(72, pig.clone(), root.position, 0.0);
            parent.extra.mount = Some(71);
            let mut child = Mob::new(73, pig, root.position, 0.0);
            child.extra.mount = Some(72);
            game.remote_entities.push(root);
            game.remote_entities.push(child);
            game.persist
                .off_level
                .insert(72, crate::game::persist::OffLevel::Mob(parent));
            for dying_passenger in [false, true] {
                if dying_passenger {
                    game.mob_object_mut(72).unwrap().health = 0.0;
                }
                game.sync_riders();
                assert_eq!(game.mob_object(72).unwrap().extra.mount, Some(71));
                assert_eq!(game.mob_object(71).unwrap().extra.rider, Some(72));
                assert_eq!(game.mob_object(73).unwrap().extra.mount, Some(72));
                assert_eq!(game.mob_object(72).unwrap().extra.rider, Some(73));
                assert_eq!(game.mob_object(73).unwrap().position.y, 200.0);
            }
        });
    }

    #[test]
    #[ignore = "daily"]
    fn synchronization_preserves_retained_controller_and_frozen_descendant() {
        crate::game::native_entity_refs::tests::with_game(|game| {
            let pig = game.res.mob_types().get("minecraft:pig").unwrap().clone();
            let mut root = Mob::new(71, pig.clone(), Vec3::new(1.0, 80.0, 0.5), 0.0);
            root.extra.rider = Some(72);
            let mut parent = Mob::new(72, pig.clone(), root.position, 0.0);
            parent.extra.mount = Some(71);
            parent.extra.rider = Some(73);
            let mut child = Mob::new(73, pig, Vec3::new(7.0, 90.0, 4.0), 0.0);
            child.extra.mount = Some(72);
            child.extra.passenger = true;
            child.extra.boarding_cooldown = 12;
            let before = child.position;
            game.remote_entities.push(root);
            game.remote_entities.push(child);
            game.persist
                .off_level
                .insert(72, crate::game::persist::OffLevel::Mob(parent));
            game.sync_riders();
            assert_eq!(game.mob_object(71).unwrap().extra.rider, Some(72));
            assert_eq!(game.mob_object(72).unwrap().extra.rider, Some(73));
            let child = game.mob_object(73).unwrap();
            assert_eq!(child.extra.mount, Some(72));
            assert!(child.extra.passenger);
            assert_eq!(child.extra.boarding_cooldown, 12);
            assert_eq!(child.position, before);
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

    struct CancelPassengerTick(Arc<AtomicBool>);
    #[mod_api::__subscriptions]
    impl mod_api::Hooks for CancelPassengerTick {
        fn entity_tick_listened(
            &self,
            _: &mut dyn mod_api::ServerHost,
            event: &mut mod_api::EntityTickListenedEvent,
        ) {
            event.listened = true;
        }
        fn entity_tick_pre(
            &self,
            _: &mut dyn mod_api::ServerHost,
            event: &mut mod_api::EntityTickPreEvent<'_>,
        ) -> mod_api::Flow {
            if event.entity == 72 && self.0.load(Ordering::Relaxed) {
                mod_api::Flow::Cancel
            } else {
                mod_api::Flow::Continue
            }
        }
    }

    #[test]
    #[ignore = "daily"]
    fn removed_vehicle_resident_retry_requires_an_eligible_uncanceled_entity_tick() {
        for cart in [false, true] {
            let cancel_mount = Arc::new(AtomicBool::new(true));
            let cancel_tick = Arc::new(AtomicBool::new(true));
            let calls = Arc::new(AtomicUsize::new(0));
            let mods: mod_api::Mods = vec![
                mod_api::ModEntry::new(
                    "cancel_mount",
                    Box::new(CancelUntilAllowed {
                        cancel: cancel_mount.clone(),
                        calls: calls.clone(),
                    }),
                ),
                mod_api::ModEntry::new(
                    "cancel_tick",
                    Box::new(CancelPassengerTick(cancel_tick.clone())),
                ),
            ]
            .into();
            mod_platform::testing::with_mods(mods, || {
                with_vehicle_tree(cart, false, |game| {
                    game.settings.simulation_distance = 4;
                    game.settings.render_distance = 12;
                    game.player = Some(
                        horizon_client::motion::PlayerMotion::new(
                            game.motion,
                            Vec3::new(8.5, 200.0, 8.5),
                        )
                        .unwrap(),
                    );
                    super::super::native_entity_refs::tests::prepare_entity_ticking_neighborhood(
                        game,
                        horizon_core::position::ChunkPos::new(0, 0),
                    );
                    game.world
                        .chunks
                        .insert((8, 0), Arc::new(horizon_client::chunk::Chunk::empty(8, 0)));
                    game.update_chunk_tickets();
                    assert!(game.entity_ticking_area()(2, 3));
                    assert!(!game.entity_ticking_area()(136, 3));
                    if cart {
                        game.hurt_cart_id(71,
                            5.0,
                            false,
                            false, None);
                    } else {
                        assert!(game.hurt_boat(0, 5.0, false));
                    }
                    assert_eq!(calls.load(Ordering::Relaxed), 1);
                    cancel_mount.store(false, Ordering::Relaxed);
                    game.mob_object_mut(72).unwrap().position = Vec3::new(136.0, 190.0, 3.0);
                    game.tick_creature_entities().unwrap();
                    assert_eq!(
                        calls.load(Ordering::Relaxed),
                        1,
                        "non-ticking resident must not retry"
                    );
                    assert_eq!(game.mob_object(72).unwrap().extra.mount, Some(71));
                    game.mob_object_mut(72).unwrap().position = Vec3::new(2.0, 190.0, 3.0);
                    game.tick_creature_entities().unwrap();
                    assert_eq!(
                        calls.load(Ordering::Relaxed),
                        1,
                        "canceled Pre must suppress retry"
                    );
                    assert_eq!(game.mob_object(72).unwrap().extra.mount, Some(71));
                    assert!(game.persist.removed_vehicles.contains_key(&71));
                    cancel_tick.store(false, Ordering::Relaxed);
                    game.tick_creature_entities().unwrap();
                    assert_eq!(calls.load(Ordering::Relaxed), 2);
                    assert_eq!(game.mob_object(72).unwrap().extra.mount, None);
                    assert!(!game.persist.removed_vehicles.contains_key(&71));
                    game.tick_creature_entities().unwrap();
                    assert_eq!(
                        calls.load(Ordering::Relaxed),
                        2,
                        "completed dismount must not replay"
                    );
                });
            });
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

    fn assert_vehicle_queries_hidden(host: &mut dyn mod_api::ServerHost, kind: &str) {
        assert!(host.entity(71).is_none());
        assert!(
            host.entity(73).is_some(),
            "removal must not hide unrelated live handles"
        );
        let level = host.level("minecraft:overworld").unwrap();
        for entities in [
            level
                .all_entities(host)
                .expect("the level remains queryable"),
            level
                .entities_in(host, [-1000.0; 3], [1000.0; 3])
                .expect("spatial queries remain available"),
        ] {
            assert!(!entities.iter().any(|entity| entity.id() == 71));
            assert!(entities.iter().any(|entity| entity.id() == 73));
        }
        assert!(
            !host
                .commands()
                .run_command(&format!("execute if entity @e[type=minecraft:{kind}]"))
        );
        assert!(
            !host
                .commands()
                .run_command("execute if entity 000000a0-0000-0000-0000-000000000047")
        );
        assert_eq!(
            host.attachment_query("vehicle_boundary", "value")
                .query(&VEHICLE_VALUE)
                .unwrap()
                .iter()
                .count(),
            0
        );
    }

    struct RemovalBoundaryObserver {
        kind: &'static str,
        cancel: Arc<AtomicBool>,
        mounts: Arc<AtomicUsize>,
        leaves: Arc<AtomicUsize>,
        destroys: Arc<AtomicUsize>,
    }
    #[mod_api::__subscriptions]
    impl mod_api::Hooks for RemovalBoundaryObserver {
        fn entity_mount(
            &self,
            host: &mut dyn mod_api::ServerHost,
            event: &mut mod_api::EntityMountEvent<'_>,
        ) -> mod_api::Flow {
            if !event.mounting && event.entity == 72 {
                self.mounts.fetch_add(1, Ordering::Relaxed);
                assert_vehicle_queries_hidden(host, self.kind);
                // A stale vehicle must not become a new parent while removal
                // is notifying the old canceled edge.
                let level = host.level("minecraft:overworld").unwrap();
                let removed = mod_api::EntityRef::new(level.epoch(), 71);
                assert!(removed.position(host).is_none());
                assert!(!host.entity(73).unwrap().start_riding(host, removed, true));
                if self.cancel.load(Ordering::Relaxed) {
                    return mod_api::Flow::Cancel;
                }
            }
            mod_api::Flow::Continue
        }
        fn entity_leave_level(
            &self,
            host: &mut dyn mod_api::ServerHost,
            event: &mut mod_api::EntityLeaveLevelEvent<'_>,
        ) {
            if event.entity == 71 {
                self.leaves.fetch_add(1, Ordering::Relaxed);
                assert_vehicle_queries_hidden(host, self.kind);
            }
        }
        fn vehicle_destroy(
            &self,
            _: &mut dyn mod_api::ServerHost,
            _: &mut mod_api::VehicleDestroyEvent<'_>,
        ) {
            self.destroys.fetch_add(1, Ordering::Relaxed);
        }
    }

    #[test]
    #[ignore = "daily"]
    fn every_vehicle_removal_boundary_hides_queries_and_retires_once() {
        // Damage, burning TNT, command kill, and deferred native removal
        // share the same visibility/retention contract for both vehicle lists.
        // Boat modes (0, 3, 5, 7) retired: their mod attachment has no ECS bridge yet.
        for mode in [1, 2, 4, 6, 8] {
            let cart = matches!(mode, 1 | 2 | 4 | 6 | 8);
            let kind = if mode == 2 {
                "tnt_minecart"
            } else if cart {
                "minecart"
            } else {
                "oak_boat"
            };
            let cancel = Arc::new(AtomicBool::new(true));
            let mounts = Arc::new(AtomicUsize::new(0));
            let leaves = Arc::new(AtomicUsize::new(0));
            let destroys = Arc::new(AtomicUsize::new(0));
            let mods: mod_api::Mods = vec![mod_api::ModEntry::new(
                "removal_boundary",
                Box::new(RemovalBoundaryObserver {
                    kind,
                    cancel: cancel.clone(),
                    mounts: mounts.clone(),
                    leaves: leaves.clone(),
                    destroys: destroys.clone(),
                }),
            )]
            .into();
            mod_platform::testing::with_mods(mods, || {
                with_vehicle_tree_of(cart.then_some(if mode == 2 { "minecraft:tnt_minecart" } else { "minecraft:minecart" }), true, |game| {
                    super::super::native_player_death::tests::add_guest(game);
                    game.native_level_opened(super::super::Dim::Overworld);
                    game.build_registries();
                    game.set_allow_commands(true);
                    let vehicle = game.entity(71).unwrap();
                    let uuid = [160, 0, 0, 71];
                    let holder = horizon_client::save::uuid_text(uuid);
                    game.persist.uuids.insert(71, uuid);
                    game.score.board.set(&holder, "boundary", 7);
                    assert!(game.native_joins_flush(false, 0));
                    match mode {
                        0 => {
                            assert!(game.hurt_boat(0, 5.0, false));
                        }
                        1 => game.hurt_cart_id(71,
                            5.0,
                            false,
                            false, None),
                        2 => {
                            game.arrow_hits_cart(71, 1.0, Vec3::ZERO, true, false, None, None);
                        }
                        3 | 4 => {
                            assert!(game.run_command(&format!("kill @e[type=minecraft:{kind}]")));
                        }
                        5 | 6 => {
                            assert!(vehicle.discard(game));
                        }
                        _ => {
                            assert!(game.entity_remove(vehicle, mod_api::RemovalReason::Killed));
                        }
                    }
                    assert_vehicle_queries_hidden(game, kind);
                    assert!(game.entity_position(vehicle).is_none());
                    assert!(game.entity_passengers(vehicle).is_none());
                    assert!(game.vehicle_by_uuid(uuid).is_none());
                    if !cart {
                        assert!(game.boat_by_uuid(uuid).is_none());
                    }
                    assert!(!vehicle.discard(game));
                    assert!(game.score.board.score(&holder, "boundary").is_none());
                    game.score.board.set(&holder, "boundary", 11);
                    for _ in 0..3 {
                        game.native_removals_flush();
                        game.native_leaves_flush();
                        assert_vehicle_queries_hidden(game, kind);
                        assert_eq!(mounts.load(Ordering::Relaxed), 1);
                        assert_eq!(leaves.load(Ordering::Relaxed), 1);
                        assert_eq!(destroys.load(Ordering::Relaxed), usize::from(mode < 2));
                        assert!(game.persist.removed_vehicles.contains_key(&71));
                        assert_eq!(game.mob_object(72).unwrap().extra.mount, Some(71));
                        assert!(game.persist.mount_notifications.is_empty());
                        game.prune_entity_memory();
                        assert_eq!(
                            game.score
                                .board
                                .score(&holder, "boundary")
                                .map(|score| score.value),
                            Some(11)
                        );
                    }
                    assert_eq!(
                        game.score
                            .board
                            .score(&holder, "boundary")
                            .map(|score| score.value),
                        Some(11)
                    );
                    game.score_destroyed_entities();
                    assert_eq!(
                        game.score
                            .board
                            .score(&holder, "boundary")
                            .map(|score| score.value),
                        Some(11)
                    );
                    cancel.store(false, Ordering::Relaxed);
                    game.dismount_mob(72);
                    assert_eq!(mounts.load(Ordering::Relaxed), 2);
                    assert_eq!(game.mob_object(72).unwrap().extra.mount, None);
                    assert!(!game.persist.removed_vehicles.contains_key(&71));
                    assert!(!game.persist.uuids.contains_key(&71));
                    assert!(!game.entity_attachments.contains_key(&71));
                    game.native_removals_flush();
                    game.dismount_mob(72);
                    assert_vehicle_queries_hidden(game, kind);
                    assert_eq!(mounts.load(Ordering::Relaxed), 2);
                    assert_eq!(leaves.load(Ordering::Relaxed), 1);
                    assert_eq!(destroys.load(Ordering::Relaxed), usize::from(mode < 2));
                    assert_eq!(
                        game.score
                            .board
                            .score(&holder, "boundary")
                            .map(|score| score.value),
                        Some(11)
                    );
                });
            });
        }
    }

    struct ObserveSpillItem(Arc<AtomicUsize>);
    #[mod_api::__subscriptions]
    impl mod_api::Hooks for ObserveSpillItem {
        fn entity_join_level(
            &self,
            host: &mut dyn mod_api::ServerHost,
            event: &mut mod_api::EntityJoinLevelEvent<'_>,
        ) -> mod_api::Flow {
            assert_ne!(
                event.entity, 71,
                "a removed vehicle must not join during its sweep"
            );
            if event.kind == "minecraft:item" {
                assert!(
                    host.entity(71).is_none(),
                    "spill callbacks cannot resolve the removed owner"
                );
                self.0.fetch_add(1, Ordering::Relaxed);
            }
            mod_api::Flow::Continue
        }
    }

    #[test]
    #[ignore = "daily"]
    fn retained_destroyed_vehicle_spills_contents_exactly_once() {
        for cart in [false, true] {
            for mode in 0..3 {
                let cancel = Arc::new(AtomicBool::new(true));
                let calls = Arc::new(AtomicUsize::new(0));
                let item_joins = Arc::new(AtomicUsize::new(0));
                let mods: mod_api::Mods = vec![
                    mod_api::ModEntry::new(
                        "cancel_spilling_vehicle",
                        Box::new(CancelUntilAllowed {
                            cancel: cancel.clone(),
                            calls: calls.clone(),
                        }),
                    ),
                    mod_api::ModEntry::new(
                        "observe_spill",
                        Box::new(ObserveSpillItem(item_joins.clone())),
                    ),
                ]
                .into();
                mod_platform::testing::with_mods(mods, || {
                    with_vehicle_tree_inner(cart.then_some("minecraft:chest_minecart"), !cart, true, |game| {
                        game.next_entity = game.next_entity.max(100);
                        let items = vec![Some(horizon_client::items::ItemStack::new(
                            "minecraft:diamond",
                            5,
                        ))];
                        if cart {
                            for (slot, stack) in items.into_iter().enumerate() {
                                game.ecs.cart_set_slot(71, slot, stack);
                            }
                        } else {
                            for (slot, stack) in items.into_iter().enumerate() {
                                game.ecs.boat_set_slot(71, slot, stack);
                            }
                        }
                        match mode {
                            0 if cart => game.hurt_cart_id(71,
                                5.0,
                                false,
                                false, None),
                            0 => {
                                assert!(game.hurt_boat(0, 5.0, false));
                            }
                            1 => game.kill_target_in_level(if cart {
                                super::super::commands::Target::Cart(71)
                            } else {
                                super::super::commands::Target::Boat(71)
                            }),
                            _ => {
                                assert!(game.entity(71).unwrap().discard(game));
                            }
                        }
                        game.native_removals_flush();
                        let diamonds = |game: &Game<'_>| -> u32 {
                            game.dropped
                                .iter()
                                .filter(|item| item.stack.item.as_ref() == "minecraft:diamond")
                                .map(|item| item.stack.count)
                                .sum()
                        };
                        assert_eq!(diamonds(game), 5);
                        assert!(item_joins.load(Ordering::Relaxed) > 0);
                        assert_eq!(calls.load(Ordering::Relaxed), 1);
                        assert!(game.persist.removed_vehicles.contains_key(&71));
                        for _ in 0..2 {
                            game.native_removals_flush();
                            game.prune_entity_memory();
                            assert_eq!(diamonds(game), 5);
                        }
                        cancel.store(false, Ordering::Relaxed);
                        game.dismount_mob(72);
                        assert!(!game.persist.removed_vehicles.contains_key(&71));
                        game.native_removals_flush();
                        assert_eq!(diamonds(game), 5);
                        assert_eq!(calls.load(Ordering::Relaxed), 2);
                    });
                });
            }
        }
    }

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
    fn destroyed_vehicles_preserve_canceled_rejected_edges_until_explicit_dismount() {
        for mode in 0..3 {
            let cancel = Arc::new(AtomicBool::new(true));
            let calls = Arc::new(AtomicUsize::new(0));
            let mods: mod_api::Mods = vec![mod_api::ModEntry::new(
                "cancel_destroyed_vehicle",
                Box::new(CancelUntilAllowed {
                    cancel: cancel.clone(),
                    calls: calls.clone(),
                }),
            )]
            .into();
            mod_platform::testing::with_mods(mods, || {
                with_vehicle_tree_of((mode != 0).then_some(if mode == 2 { "minecraft:tnt_minecart" } else { "minecraft:minecart" }), true, |game| {
                    let uuid = [160, 0, 0, 71];
                    let holder = horizon_client::save::uuid_text(uuid);
                    game.persist.uuids.insert(71, uuid);
                    game.score.board.set(&holder, "retained", 7);
                    if mode == 0 {
                        assert!(game.hurt_boat(0, 5.0, false));
                    } else if mode == 1 {
                        game.hurt_cart_id(71,
                            5.0,
                            false,
                            false, None);
                    } else {
                        game.arrow_hits_cart(71, 1.0, Vec3::ZERO, true, false, None, None);
                    }
                    assert_eq!(calls.load(Ordering::Relaxed), 1);
                    assert!(game.entity(71).is_none());
                    assert!(game.entity(72).is_none());
                    assert!(game.entity(73).is_some());
                    assert!(game.persist.removed_vehicles.contains_key(&71));
                    assert!(game.score.board.score(&holder, "retained").is_none());
                    game.score.board.set(&holder, "retained", 11);
                    game.score_destroyed_entities();
                    assert_eq!(
                        game.score.board.score(&holder, "retained").map(|s| s.value),
                        Some(11)
                    );
                    assert_eq!(game.mob_object(72).unwrap().extra.mount, Some(71));
                    assert_eq!(game.mob_object(73).unwrap().position, Vec3::ZERO);
                    game.sync_riders();
                    assert_eq!(
                        calls.load(Ordering::Relaxed),
                        1,
                        "rejected riders do not tick"
                    );
                    assert_eq!(game.mob_object(73).unwrap().age, 19);
                    cancel.store(false, Ordering::Relaxed);
                    game.dismount_mob(72);
                    assert_eq!(calls.load(Ordering::Relaxed), 2);
                    assert_eq!(game.mob_object(72).unwrap().extra.mount, None);
                    assert_eq!(
                        game.mob_object(72).unwrap().position,
                        Vec3::new(2.0, 200.0, 3.0)
                    );
                    let child = game.mob_object(73).unwrap();
                    assert_eq!(child.extra.mount, Some(72));
                    assert_eq!(child.position.x, 2.0);
                    assert_eq!(child.position.z, 3.0);
                    assert!((child.position.y - 200.86875).abs() < 1e-6);
                    assert!(!game.persist.removed_vehicles.contains_key(&71));
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
    struct ReparentLater(Arc<AtomicUsize>);
    #[mod_api::__subscriptions]
    impl mod_api::Hooks for ReparentLater {
        fn entity_mount(
            &self,
            host: &mut dyn mod_api::ServerHost,
            event: &mut mod_api::EntityMountEvent<'_>,
        ) -> mod_api::Flow {
            if !event.mounting && event.entity == 73 {
                self.0.fetch_add(1, Ordering::Relaxed);
            }
            if !event.mounting && event.entity == 72 {
                let later = host.entity(73).unwrap();
                let replacement = host.entity(74).unwrap();
                assert!(later.start_riding(host, replacement, true));
                return mod_api::Flow::Cancel;
            }
            mod_api::Flow::Continue
        }
    }
    #[test]
    #[ignore = "daily"]
    fn destruction_preserves_later_rider_reparenting_without_an_extra_notification() {
        for mode in 0..5 {
            let cart = mode >= 2;
            let count = Arc::new(AtomicUsize::new(0));
            let mods: mod_api::Mods = vec![mod_api::ModEntry::new(
                "reparent_cleanup",
                Box::new(ReparentLater(count.clone())),
            )]
            .into();
            mod_platform::testing::with_mods(mods, || {
                with_vehicle_tree_of(cart.then_some(if mode == 4 { "minecraft:tnt_minecart" } else { "minecraft:minecart" }), false, |game| {
                    game.mob_object_mut(72).unwrap().extra.vehicle_order = 1;
                    let later = game.mob_object_mut(73).unwrap();
                    later.extra.mount = Some(71);
                    later.extra.vehicle_order = 2;
                    let kind = game.res.mob_types().get("minecraft:cow").unwrap().clone();
                    game.remote_entities.push(Mob::new(74, kind, Vec3::ZERO, 0.0));
                    match mode {
                        0 => game.kill_target_in_level(super::super::commands::Target::Boat(71)),
                        1 => {
                            assert!(game.hurt_boat(0, 5.0, false));
                        }
                        2 => game.kill_target_in_level(super::super::commands::Target::Cart(71)),
                        3 => game.hurt_cart_id(71,
                            5.0,
                            false,
                            false, None),
                        _ => {
                            game.arrow_hits_cart(71, 1.0, Vec3::ZERO, true, false, None, None);
                        }
                    }
                    assert_eq!(game.mob_object(72).unwrap().extra.mount, Some(71));
                    assert_eq!(game.mob_object(73).unwrap().extra.mount, Some(74));
                    assert_eq!(
                        count.load(Ordering::Relaxed),
                        1,
                        "only deliberate reparenting dismounts the later rider"
                    );
                    game.native_removals_flush();
                    assert_eq!(game.mob_object(73).unwrap().extra.mount, Some(74));
                    assert_eq!(count.load(Ordering::Relaxed), 1);
                    assert!(game.persist.mount_notifications.is_empty());
                });
            });
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

#[cfg(test)]
mod trap_scatter_tests {
    use super::*;
    use horizon_core::random::RandomSource;
    /// `EnchantmentsByCostWithDifficulty.enchant` reads the stack the mob
    /// holds: an empty slot draws nothing from the world's random.
    #[test]
    #[ignore = "daily"]
    fn enchant_item_on_an_empty_slot_draws_nothing() {
        let mut resources = None;
        let mut game = super::super::game_tests::tests::server_game(&mut resources);
        let id = game
            .spawn_mob_raw("minecraft:skeleton_horse", [8.5, 64.0, 8.5], false)
            .unwrap();
        let seeded = || {
            horizon_core::random::LevelRandom::for_level(horizon_core::random::LegacyRandom::new(5))
        };
        game.world.random = seeded();
        game.enchant_item(id, horizon_client::mobs::finalize::MAINHAND, 0.5);
        assert_eq!(game.world.random.next_long(), seeded().next_long());
    }
}
