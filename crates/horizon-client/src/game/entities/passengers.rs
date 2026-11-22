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

impl Game<'_> {
    /// `Entity.startRiding` for two mobs.
    pub(super) fn mount_mob(&mut self, rider: u32, vehicle: u32) {
        self.try_mount_mob(rider, vehicle);
    }
    /// `Entity.startRiding` for two mobs: false when a mod canceled it.
    pub(super) fn try_mount_mob(&mut self, rider: u32, vehicle: u32) -> bool {
        // `start_riding_target` asks the mod hooks, then mounts the ECS rider (its `Mob` arm).
        let vehicle = if self.ecs.boat_status(vehicle).is_some() || self.is_boat_id(vehicle) {
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
        if !self.ecs_mount(rider, vehicle) {
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
        // The ECS seats its mob riders (`mount` systems).
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
                    self.passenger_riding_position(v, me, own_seat)
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
        for stand in self.armor_stand_list() {
            let Some(vehicle) = stand.vehicle else {
                continue;
            };
            let Some(at) = self.passenger_riding_position(vehicle, me, own_seat) else {
                continue;
            };
            // The ECS keeps no vehicle's previous seat: the stand keeps its own.
            let before = stand.previous;
            let at = Vec3::new(at.x, at.y, at.z);
            self.armor_stand_edit(stand.id, |a| {
                a.position = at;
                a.previous = before;
                a.velocity = Vec3::ZERO;
            });
        }
    }

    /// `Entity.getPassengerRidingPosition` of a vehicle: where the feet of a
    /// passenger whose own vehicle attachment is 0 land.
    pub(super) fn passenger_riding_position(
        &self,
        vehicle: u32,
        me: Option<u32>,
        own_seat: Option<Vec3>,
    ) -> Option<glam::DVec3> {
        self.passenger_riding_position_inner(vehicle, me, own_seat, &mut Vec::new())
    }

    fn passenger_riding_position_inner(
        &self,
        vehicle: u32,
        me: Option<u32>,
        own_seat: Option<Vec3>,
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
        // Gap: the vehicle's passenger attachment (per species) is not a P component yet; the feet stand in for it.
        if let Some(p) = self.ecs.position(vehicle) {
            return Some(glam::DVec3::new(p[0], p[1], p[2]));
        }
        if let Some(d) = self.text_displays.get(&(vehicle as i32)) {
            return if let Some(parent) = d.vehicle {
                self.passenger_riding_position_inner(parent, me, own_seat, path)
            } else {
                Some(d.position())
            };
        }
        if let Some(i) = self.interactions.get(&(vehicle as i32)) {
            if let Some(parent) = i.vehicle {
                return self.passenger_riding_position_inner(parent, me, own_seat, path);
            }
            let at = i.position;
            return Some(glam::DVec3::new(at.x, at.y, at.z));
        }
        if self.is_boat_id(vehicle) {
            let count = self.boat_players(vehicle).max(1) + self.boat_mobs(vehicle).len();
            if let Some(at) = self.ecs.boat_seat(vehicle, 0, count) {
                return Some(glam::DVec3::new(at.x, at.y, at.z));
            }
        }
        if let Some(a) = self.armor_stand_saved(vehicle) {
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
                self.passenger_riding_position_inner(parent, me, own_seat, path)?
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
