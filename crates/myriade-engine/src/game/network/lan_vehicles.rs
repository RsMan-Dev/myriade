//! Boats and minecarts for a server's players: LAN guests, players on a
//! dedicated server, and single player's window on its server thread.
//!
//! A player aims at a vehicle, item frame, armor stand or painting as at a
//! mob (`ServerGamePacketListenerImpl.handleInteract`), and the game's own
//! code for it runs as that player (`AbstractBoat.interact` seats it,
//! `VehicleEntity.hurtServer` breaks the vehicle). A boat is driven by its
//! rider's client (`handleMoveVehicle`, `handlePaddleBoat`); a minecart moves
//! here with the rider's input (`ServerPlayer.getLastClientMoveIntent`).
//! Sneaking gets out (`Player.rideTick`: `wantsToStopRiding`), and the client
//! is teleported where the player lands (`ServerPlayer.dismountTo`).
use super::{Dim, Game, Keys, lan::LanHost, minecarts::SEAT};
use myriade_engine::collisions::SceneCollisions;
use myriade_engine::lan::hub::no_blocks_in;
use myriade_engine::mobs::equine;
use horizon_core::geometry::{Aabb, Axis, Vec3};
use horizon_protocol::{
    packets::play::{self, Clientbound as C},
    text::translatable,
};
use horizon_world::entity_move::MoverType;

/// `PlayerInput` flags, as the server player's client last sent them.
pub(super) fn input_keys(flags: u8) -> Keys {
    Keys {
        forward: flags & 1 != 0,
        backward: flags & 2 != 0,
        left: flags & 4 != 0,
        right: flags & 8 != 0,
        jump: flags & 16 != 0,
        shift: flags & 32 != 0,
        sprint: flags & 64 != 0,
        ..Keys::default()
    }
}

fn corners(b: Aabb) -> ([f64; 3], [f64; 3]) {
    let (lo, hi) = (b.min_position(), b.max_position());
    ([lo.x, lo.y, lo.z], [hi.x, hi.y, hi.z])
}

/// A boat or minecart a server player rode, followed for
/// `native_vehicle_moved`.
pub(super) struct WatchedVehicle {
    id: u32,
    dim: Dim,
    /// The server player it carries now.
    rider: Option<u128>,
    /// Where the hook last heard of it.
    at: [f64; 3],
    /// Ticks it has stood still.
    still: u32,
}

impl Game<'_> {
    /// Where the boat or minecart `id` of this level is.
    fn watched_vehicle_position(&self, id: u32) -> Option<[f64; 3]> {
        (self.is_boat_id(id) || self.cart_genre(id).is_some()).then(|| self.ecs.position(id)).flatten()
    }

    /// `VehicleMoveEvent` for the boats and minecarts server players ride
    /// (and left less than ten seconds ago): once a tick, after they moved.
    pub(super) fn tick_vehicle_watch(&mut self) {
        let riders: Vec<(u32, u32, u128)> = self.lan.as_ref().map_or_else(Vec::new, |h| {
            h.hub
                .guests
                .iter()
                .filter_map(|g| Some((g.vehicle?, g.conn, g.uuid)))
                .collect()
        });
        let mut watched = self.lan.as_mut().map_or_else(Vec::new, |h| std::mem::take(&mut h.watched_vehicles));
        if riders.is_empty() && watched.is_empty() {
            return;
        }
        let dim = self.dim;
        for &(id, _, uuid) in &riders {
            let Some(at) = self.watched_vehicle_position(id) else {
                continue;
            };
            match watched.iter_mut().find(|w| w.id == id) {
                Some(w) => (w.dim, w.rider) = (dim, Some(uuid)),
                None => watched.push(WatchedVehicle {
                    id,
                    dim,
                    rider: Some(uuid),
                    at,
                    still: 0,
                }),
            }
        }
        let mut kept = Vec::new();
        // (rider's connection and UUID, vehicle type, from, to)
        let mut moved = Vec::new();
        for mut w in std::mem::take(&mut watched) {
            // Another level's vehicle is told of on that level's tick.
            if w.dim != dim {
                kept.push(w);
                continue;
            }
            // Broken, or gone from this level.
            let Some(now) = self.watched_vehicle_position(w.id) else {
                continue;
            };
            let rider = riders.iter().find(|r| r.0 == w.id);
            w.rider = rider.map(|r| r.2);
            if now != w.at {
                let from = std::mem::replace(&mut w.at, now);
                w.still = 0;
                let kind = self.target_type(
                    self.level_entity(w.id as i32)
                        .unwrap_or(super::commands::Target::Boat(w.id)),
                );
                moved.push((rider.map(|r| (r.1, r.2)), kind, from, now));
            } else {
                w.still += 1;
                if w.rider.is_none() && w.still > 200 {
                    continue;
                }
            }
            kept.push(w);
        }
        if let Some(h) = self.lan.as_mut() {
            h.watched_vehicles = kept;
        }
        // The rider's state is swapped in for the call, so the helpers reach
        // them; a vehicle nobody rides is told of with nobody swapped in.
    }

    /// The box of a boat, minecart, armor stand, item frame or painting, and
    /// the point an interact packet's location is measured from (the
    /// client's `remote_object_hit` uses the same points).
    pub(super) fn object_target(&self, id: i32) -> Option<([f64; 3], [f64; 3], [f64; 3])> {
        let at = |v: Vec3| [v.x, v.y, v.z];
        let centre = |p: [i32; 3]| p.map(|v| f64::from(v) + 0.5);
        if let Some(i) = self.interactions.get(&id) {
            let (lo, hi) = corners(i.bounds());
            return Some((lo, hi, at(i.position)));
        }
        let id = u32::try_from(id).ok()?;
        if self.is_boat_id(id) || self.cart_genre(id).is_some() {
            let (lo, hi) = corners(self.ecs.bounds(id)?);
            return Some((lo, hi, self.ecs.position(id)?));
        }
        if let Some(a) = self.armor_stand_saved(id) {
            let (lo, hi) = corners(a.bounds());
            return Some((lo, hi, at(a.position)));
        }
        if let Some(c) = self.cushion_saved(id) {
            let (lo, hi) = corners(Aabb::new(
                Vec3::new(c.position.x - 0.5, c.position.y, c.position.z - 0.5),
                Vec3::new(c.position.x + 0.5, c.position.y + 0.25, c.position.z + 0.5),
            ));
            return Some((lo, hi, at(c.position)));
        }
        if let Some(f) = self.ecs.entity_of(id).and_then(|e| self.frame_from_ecs(e)) {
            let (lo, hi) = f.bounds();
            return Some((lo, hi, centre(f.pos)));
        }
        let p = self.ecs.entity_of(id).and_then(|e| self.painting_from_ecs(e))?;
        let (lo, hi) = p.bounds();
        Some((lo, hi, centre(p.pos)))
    }

    /// The vehicles the server's players ride: they move with their riders,
    /// not in `tick_boats` and `tick_carts`.
    pub(super) fn guest_vehicles(&self) -> Vec<u32> {
        self.lan.as_ref().map_or_else(Vec::new, |h| {
            h.hub.guests.iter().filter_map(|g| g.vehicle).collect()
        })
    }

    /// Whether a server player other than the one acting rides `vehicle`
    /// (`Entity.canAddPassenger`: a boat or minecart seats one player here).
    pub(super) fn ridden_by_guest(&self, vehicle: u32) -> bool {
        self.lan.as_ref().is_some_and(|h| {
            h.hub
                .guests
                .iter()
                .any(|g| Some(g.conn) != self.lan_acting && g.vehicle == Some(vehicle))
        })
    }
    /// Whether a server player rides `vehicle` (`Entity.isPassenger`: a
    /// passenger does not use a portal on its own, the vehicle carries it
    /// across, `Entity.teleportCrossDimension`).
    pub(super) fn guest_rides(&self, vehicle: u32) -> bool {
        self.lan
            .as_ref()
            .is_some_and(|h| h.hub.guests.iter().any(|g| g.vehicle == Some(vehicle)))
    }

    /// `Player.rideTick` for a server player, before its moves this tick:
    /// it stays in its seat, steers its minecart, or gets out.
    pub(super) fn guest_vehicle_tick(&mut self, conn: u32) {
        let Some(host) = self.lan.as_ref() else {
            return;
        };
        let Some(gi) = host.hub.guests.iter().position(|g| g.conn == conn) else {
            return;
        };
        let g = &host.hub.guests[gi];
        let Some(vehicle) = g.vehicle else {
            return;
        };
        let (keys, dead) = (input_keys(g.input), g.dead || g.health <= 0.0);
        let boat = self.ecs.boat_status(vehicle).is_some();
        let cart = self.ecs.cart_kind(vehicle).is_some();
        let mount = self.mob_exists(vehicle);
        let cushion = self.cushion_ids().contains(&vehicle);
        let seat = self.seat(vehicle).is_some();
        // ServerGamePacketListenerImpl.tick: the vehicle's last good position
        // is where it is now, so a move the server made (a piston, a push)
        // is not held against the rider's next move, which would fail, be
        // sent back, and fail again on every answer.
        let at = boat
            .then(|| self.ecs.position(vehicle).map(|p| Vec3::new(p[0], p[1], p[2])))
            .flatten()
            .or_else(|| self.mount_steered_at(vehicle));
        let Some(host) = self.lan.as_mut() else {
            return;
        };
        // The vehicle is gone (broken, in another level) or the rider died:
        // its client drops it with the entity or the death screen.
        if dead || (!boat && !cart && !mount && !cushion && !seat) {
            host.hub.guests.update(gi, |g| {
                (g.vehicle, g.vehicle_good) = (None, None);
            });
            return;
        }
        if let Some(p) = at {
            {
                let value = Some([p.x, p.y, p.z]);
                host.hub
                    .guests
                    .update(gi, |guest| guest.vehicle_good = value);
            }
        }
        // Getting out goes through leave_guest, which tells the client and
        // teleports it (ServerPlayer.dismountTo).
        let seat = self.as_guest(conn, |game| {
            // A cushion does not move: the rider sits on it until it sneaks
            // (`Cushion.removePassenger` plays the get-up sound).
            if game.riding.is_some_and(|id| game.cushion_ids().contains(&id)) {
                if keys.shift {
                    game.tick_cushion_riding(keys);
                    return None;
                }
                let c = game.riding.and_then(|id| game.cushion_saved(id))?;
                let p = c.seat();
                return Some([p.x, p.y, p.z]);
            }
            // An armor stand, a display, a marker...: the rider keeps its seat as the vehicle is
            // moved (/tp, a mod) until it sneaks.
            if game.riding.is_some_and(|id| game.seat(id).is_some()) {
                let p = game.tick_seat_riding(keys)?;
                return Some([p.x, p.y, p.z]);
            }
            if let Some(id) = game.riding.filter(|&id| game.ecs.boat_status(id).is_some()) {
                if keys.shift {
                    game.dismount_boat(id);
                    return None;
                }
                let p = game.player_boat_seat(id);
                return Some([p.x, p.y, p.z]);
            }
            // The minecart moves with the rider's input, and lets it out when
            // it sneaks or the cart breaks. An animal takes the rider's input
            // for its own tick (`travelRidden`), or lets it off.
            if !game.tick_cart_riding(keys) && game.tick_mount_riding(keys) {
                game.riding?;
                game.position_rider();
            }
            game.riding?;
            let p = game.player.as_ref()?.position;
            Some([p.x, p.y, p.z])
        });
        if let Some(Some(at)) = seat
            && let Some(host) = self.lan.as_mut()
        {
            host.hub.guests.update(gi, |g| {
                g.previous_position = g.position;
                g.position = at;
            });
        }
    }

    /// After an interaction: a player that got into a vehicle sits in it,
    /// and its last good vehicle position is where the vehicle is.
    pub(super) fn guest_boarded(&mut self, conn: u32, before: Option<u32>) {
        let Some(host) = self.lan.as_mut() else {
            return;
        };
        let Some(gi) = host.hub.guests.iter().position(|g| g.conn == conn) else {
            return;
        };
        let Some(vehicle) = host.hub.guests[gi].vehicle else {
            return;
        };
        if before == Some(vehicle) {
            return;
        }
        // The host in the window rides it already.
        if self.riding == Some(vehicle) {
            {
                let value = before;
                host.hub.guests.update(gi, |guest| guest.vehicle = value);
            }
            return;
        }
        let at = self
            .ecs
            .boat_status(vehicle)
            .and_then(|_| self.ecs_view(vehicle))
            .map(|v| (Vec3::new(v.position[0], v.position[1], v.position[2]), self.player_boat_seat(vehicle)))
            .or_else(|| {
                self.cart_genre(vehicle)?;
                let p = self.ecs.position(vehicle).map(|p| Vec3::new(p[0], p[1], p[2]))?;
                Some((p, p.add_vector(Vec3::new(0.0, SEAT, 0.0))))
            })
            .or_else(|| {
                self.cushion_saved(vehicle).map(|c| (c.position, c.seat()))
            })
            .or_else(|| self.seat(vehicle).map(|s| (s.position, s.player_seat())));
        if let Some((p, seat)) = at {
            let Some(host) = self.lan.as_mut() else {
                return;
            };
            host.hub.guests.update(gi, |g| {
                g.vehicle_good = Some([p.x, p.y, p.z]);
                g.previous_position = g.position;
                g.position = [seat.x, seat.y, seat.z];
                g.on_ground = false;
            });
            return;
        }
        // An animal: the rider sits where `positionRider` puts it.
        let seat = self.as_guest(conn, |game| {
            game.position_rider();
            game.player
                .as_ref()
                .map(|p| [p.position.x, p.position.y, p.position.z])
        });
        if let Some(Some(at)) = seat
            && let Some(host) = self.lan.as_mut()
        {
            host.hub.guests.update(gi, |g| {
                g.previous_position = g.position;
                g.position = at;
                g.on_ground = false;
            });
        }
    }

    /// `ServerGamePacketListenerImpl.handleMoveVehicle`: the boat or the
    /// saddled horse its rider drives goes where the client says, unless it
    /// moved too quickly or wrongly; then it stays, and the client is told
    /// where it is.
    pub(super) fn guest_move_vehicle(&mut self, conn: u32, p: &play::MoveVehicleRequest) {
        let controls_vehicle = self
            .lan
            .as_ref()
            .and_then(|host| host.hub.guest(conn))
            .and_then(|g| g.vehicle)
            .is_none_or(|v| self.guest_controls_vehicle(conn, v));
        let Some(host) = self.lan.as_ref() else {
            return;
        };
        let Some(gi) = host.hub.guests.iter().position(|g| g.conn == conn) else {
            return;
        };
        let [x, y, z] = p.position;
        if contains_invalid_values(p.position, p.yaw, p.pitch) {
            self.lan.as_mut().unwrap().hub.kick(
                conn,
                translatable("multiplayer.disconnect.invalid_vehicle_movement", &[]),
            );
            return;
        }
        let g = &host.hub.guests[gi];
        if g.awaiting_teleport.is_some() || !g.client_loaded() {
            return;
        }
        // Only the vehicle's controlling passenger moves it: the move of a
        // player in a boat's second seat is ignored (#11541).
        if !controls_vehicle {
            return;
        }
        let owner = host.hub.singleplayer_owner == Some(g.uuid);
        let allow_flight = host.hub.config.allow_flight;
        let (vehicle, good) = (g.vehicle, g.vehicle_good);
        let back = |host: &mut LanHost, position: Vec3, yaw: f32| {
            let packet = play::MoveVehicle {
                position: [position.x, position.y, position.z],
                yaw,
                pitch: 0.0,
            };
            host.hub.send_to(conn, &C::MoveVehicle(packet));
        };
        // Only a boat, and a horse its rider's client moves, are driven by
        // their rider (getControllingPassenger); a minecart that is elsewhere
        // on the client is put back.
        let boat = vehicle.filter(|&v| self.ecs.boat_status(v).is_some());
        if boat.is_none()
            && let Some(v) = vehicle
            && self.mount_moved_by_client(v)
        {
            let target = Vec3::new(x, y, z);
            // The player's own move gate (`.clamp`) is the boat's below; here the rider's client is the authority.
            let target = Vec3::new(target.x.clamp(-3.0E7, 3.0E7), target.y.clamp(-2.0E7, 2.0E7), target.z.clamp(-3.0E7, 3.0E7));
            if self.ecs_mount_move(conn, v, target, p.yaw, p.pitch, owner) {
                // `lastGoodX/Y/Z` of the vehicle: where the accepted move left it.
                self.lan.as_mut().unwrap().hub.guests.update(gi, |g| {
                    g.release_tickets();
                    g.vehicle_good = Some([target.x, target.y, target.z]);
                });
            }
            return;
        }
        // The vehicle's id, kind, position, rotation, motion, box, step height
        // and ground.
        let (id, old, yaw, velocity, bounds, step, on_ground) = if let Some(v) = boat {
            let (Some(m), Some(bounds)) = (self.ecs.motion(v), self.ecs.bounds(v)) else {
                return;
            };
            (
                v,
                Vec3::new(m.position[0], m.position[1], m.position[2]),
                m.yaw,
                Vec3::new(m.velocity[0], m.velocity[1], m.velocity[2]),
                bounds,
                0.0,
                m.on_ground,
            )
        } else {
            if let Some(v) = vehicle
                && let Some(m) = self.ecs.motion(v).filter(|_| self.ecs.cart_kind(v).is_some())
                && Vec3::new(m.position[0], m.position[1], m.position[2]).distance_squared(Vec3::new(x, y, z)) > 1.0
            {
                back(self.lan.as_mut().unwrap(), Vec3::new(m.position[0], m.position[1], m.position[2]), m.yaw);
            }
            return;
        };
        let good = good.map_or(old, |g| Vec3::new(g[0], g[1], g[2]));
        let target = Vec3::new(
            x.clamp(-3.0E7, 3.0E7),
            y.clamp(-2.0E7, 2.0E7),
            z.clamp(-3.0E7, 3.0E7),
        );
        // Moved too quickly: more than 10 blocks past its own motion.
        let moved = target.distance_squared(old) - velocity.length_squared();
        if moved > 100.0 && !owner {
            log::warn!("vehicle {id} moved too quickly! {moved}");
            back(self.lan.as_mut().unwrap(), old, yaw);
            return;
        }
        // vehicle.move(PLAYER, target - lastGood) with the vehicle's
        // collisions: where it can't follow the client (a quarter block off,
        // the height aside), it moved wrongly. A Player-cause move through
        // the move gate (arismoko/horizon#2414); PR 1a records the movement unchanged.
        let mut scene = SceneCollisions {
            world: &self.world,
            binding: self.collision,
            subject: None,
        };
        let wanted = target.subtract(good);
        let mut gate = myriade_engine::move_gate::armed(MoverType::Player, wanted);
        let wanted = gate.delta();
        // A listener's decision that is not the request (arismoko/horizon#2414, ruling 5):
        // the driver is put back whatever the distance - the flag comes from
        // the gate, not from the numbers.
        let overridden = gate.overridden();
        let resolved = myriade_engine::move_gate::scene_resolve(
            &mut gate, &mut scene, bounds, wanted, step, on_ground,
        )
        .unwrap_or(wanted);
        let reached = old.add_vector(resolved);
        let wrongly =
            overridden || (target.x - reached.x).powi(2) + (target.z - reached.z).powi(2) > 0.0625;
        let free_before = scene.unobstructed(bounds).unwrap_or(true);
        // isEntityCollidingWithAnythingNew: a box that was free is not there.
        let moved_box = bounds.move_by(target.subtract(old));
        let free_after = scene
            .unobstructed(moved_box.deflate(Vec3::new(1.0E-5, 1.0E-5, 1.0E-5)))
            .unwrap_or(true);
        // Entity.move in `vehicle.move`, before the checks: a long move through
        // water or a reset block resets the fall, which a move put back keeps
        // (`absSnapTo` restores only the position).
        if let Some(v) = boat
            && let Some(e) = self.ecs.entity_of(v)
        {
            let reset = self.vehicle_fall_reset(self.ecs.fall_distance(v), old, resolved);
            self.ecs.set_fall_distance(e, reset);
        }
        // An overridden decision is put back from wherever the vehicle starts:
        // the free-old-box exemption is the distance test's (review round 1),
        // like the hub's unobstructed-start check.
        if overridden || (free_before && (wrongly || !free_after)) {
            log::warn!("vehicle {id} moved wrongly");
            back(self.lan.as_mut().unwrap(), old, yaw);
            return;
        }
        // clientVehicleIsFloating: the move didn't go down a step, the vehicle hadn't come down onto
        // something on its last move (`verticalCollisionBelow` before this one), and nothing but air is
        // around its box grown by 0.0625 and stretched 0.55 down (water holds a boat up). A server that allows flight never floats.
        let gravity = 0.04;
        let around = moved_box
            .inflate(Vec3::new(0.0625, 0.0625, 0.0625))
            .expand_towards(Vec3::new(0.0, -0.55, 0.0));
        let corner = |max: bool| {
            [Axis::X, Axis::Y, Axis::Z].map(|a| if max { around.max(a) } else { around.min(a) })
        };
        let no_blocks = no_blocks_in(
            &self.world,
            corner(false),
            corner(true),
            &self.lan.as_ref().unwrap().hub.encoder,
        );
        self.lan.as_mut().unwrap().hub.guests.update(gi, |g| {
            // `ServerChunkCache.move` after `absSnapTo` of the accepted move.
            g.release_tickets();
            g.vehicle_good = Some([target.x, target.y, target.z]);
            let rested = std::mem::replace(
                &mut g.vehicle_rests,
                wanted.y < 0.0 && resolved.y != wanted.y,
            );
            g.vehicle_floating =
                wanted.y >= -0.03125 && !rested && !allow_flight && no_blocks;
            g.vehicle_gravity = gravity;
        });
        let (Some(v), Some(e)) = (boat, boat.and_then(|v| self.ecs.entity_of(v))) else {
            return;
        };
        // absSnapTo of the accepted move; P holds no setter for the ground flag of a boat (listed in the report).
        self.ecs_with(|ecs, mw| {
            ecs.teleport(mw, e, [target.x, target.y, target.z]);
            ecs.set_rotation(e, p.yaw, 0.0);
            ecs.set_velocity(e, target.subtract(old));
        });
        // handleMoveVehicle's `vehicle.move` counts the fall (AbstractBoat.checkFallDamage); the server's
        // baseTick resets it in water, which this boat's skipped tick doesn't run.
        let in_water = self.ecs.boat_status(v).is_some_and(|s| s.in_liquid);
        let fall = if in_water {
            0.0
        } else {
            self.boat_fall_at(target, p.on_ground, self.ecs.fall_distance(v), resolved.y)
        };
        self.ecs.set_fall_distance(e, fall);
        let seat = self.player_boat_seat(v);
        let Some(host) = self.lan.as_mut() else {
            return;
        };
        host.hub.guests.update(gi, |g| {
            g.previous_position = g.position;
            g.position = [seat.x, seat.y, seat.z];
        });
    }

    /// `Entity.getControllingPassenger` of a vehicle a server player rides: a
    /// boat has two seats, and only its first player passenger drives it. The
    /// window player sits first when it rides; of the server's players it is
    /// the one that got in first.
    pub(super) fn guest_controls_vehicle(&self, conn: u32, vehicle: u32) -> bool {
        let Some(host) = self.lan.as_ref() else {
            return false;
        };
        if self.riding == Some(vehicle) {
            return false;
        }
        host.hub
            .guests
            .iter()
            .filter(|g| g.vehicle == Some(vehicle))
            .min_by_key(|g| g.vehicle_order)
            .is_some_and(|g| g.conn == conn)
    }

    /// `handlePaddleBoat`: the paddles of the boat the player controls
    /// (`player.getControlledVehicle()`); a passenger in the second seat moves
    /// no paddle.
    pub(super) fn guest_paddle_boat(&mut self, conn: u32, paddles: [bool; 2]) {
        let Some(host) = self.lan.as_ref() else {
            return;
        };
        let vehicle = host
            .hub
            .guests
            .iter()
            .find(|g| g.conn == conn)
            .and_then(|g| g.vehicle);
        let Some(vehicle) = vehicle.filter(|&v| self.guest_controls_vehicle(conn, v)) else {
            return;
        };
        self.ecs.boat_paddle(vehicle, paddles);
    }
}

/// `ServerGamePacketListenerImpl.containsInvalidValues(DDDFF)`: a NaN coordinate or a rotation
/// that is not finite. An infinite coordinate is valid there: `handleMoveVehicle` clamps it and
/// goes on with the usual checks, so it does not disconnect.
fn contains_invalid_values([x, y, z]: [f64; 3], yaw: f32, pitch: f32) -> bool {
    x.is_nan() || y.is_nan() || z.is_nan() || !yaw.is_finite() || !pitch.is_finite()
}

#[cfg(test)]
mod controller_tests {
    /// vanilla's `containsInvalidValues`: only a NaN coordinate or a non-finite rotation is invalid.
    #[test]
    fn only_a_nan_coordinate_or_a_non_finite_rotation_is_invalid() {
        use super::contains_invalid_values as invalid;
        assert!(!invalid([1.0, 64.0, -3.5], 90.0, 10.0));
        for at in [f64::INFINITY, f64::NEG_INFINITY] {
            assert!(!invalid([at, 64.0, 0.0], 0.0, 0.0), "x {at}");
            assert!(!invalid([0.0, at, 0.0], 0.0, 0.0), "y {at}");
            assert!(!invalid([0.0, 64.0, at], 0.0, 0.0), "z {at}");
        }
        assert!(invalid([f64::NAN, 64.0, 0.0], 0.0, 0.0));
        assert!(invalid([0.0, f64::NAN, 0.0], 0.0, 0.0));
        assert!(invalid([0.0, 64.0, f64::NAN], 0.0, 0.0));
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert!(invalid([0.0, 64.0, 0.0], bad, 0.0), "yaw {bad}");
            assert!(invalid([0.0, 64.0, 0.0], 0.0, bad), "pitch {bad}");
        }
    }

}
