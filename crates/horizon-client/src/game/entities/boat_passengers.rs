//! Boats and the entities around them: what a boat runs into
//! (`AbstractBoat.canVehicleCollide`), the pushes between a boat and what
//! touches it (`AbstractBoat.push`, `LivingEntity.pushEntities`), the mobs
//! it picks up (the end of `AbstractBoat.tick`), where they sit
//! (`positionRider`, `clampRotation`) and how they get out.
//! Saving them under the boat's `Passengers` lives in `persist.rs`.
use super::{Game, boats::Boat, lan_mobs::guest_bounds};
use horizon_client::{
    fast_hash::FxHashMap,
    join_seq::ListStamp,
    mobs::{BoxReach, Mob, Species, Who, riders, seen_grid::SeenGrid},
    scoreboard::pushable_by,
};
use horizon_core::geometry::{Aabb, Vec3};
use std::cell::{Cell, RefCell};

/// `Entity.boardingCooldown` once a mob leaves its boat.
pub(super) const BOARDING_COOLDOWN: i32 = 60;
/// `Player.DEFAULT_VEHICLE_ATTACHMENT`: how far below the seat a player's
/// feet are.
const PLAYER_RIDING_OFFSET: f64 = 0.6;

/// How many box queries the walk over every mob answers, once a write to the mob list has
/// made the grid stale, before a new grid is built: about what building one costs (with
/// 25,000 mobs, six to eight walks). It doubles with each grid of a stretch, so vehicles
/// that each write a mob (a boat pushing the pig beside it) never pay more for grids than
/// for their walks.
const WALKS_BEFORE_GRID: u32 = 8;

/// What a stretch of vehicle ticks knows about the mobs, so a boat or a minecart reads the
/// mobs near it and not every mob of the level ([`Game::mobs_meeting`]).
#[derive(Default)]
pub(in crate::game) struct VehicleMobs {
    /// The mobs' [`BoxReach`] with the mob count it was worked out for; None outside a stretch.
    reach: Cell<Option<(usize, BoxReach)>>,
    grid: RefCell<MobGrid>,
    /// The last query's candidates, kept for their allocation.
    near: Cell<Vec<u32>>,
    /// Each ridden vehicle's mobs by slot, for the mob list as it stood ([`Game::vehicle_riders`]).
    riders: RefCell<Option<(ListStamp, FxHashMap<u32, Vec<usize>>)>>,
}
#[derive(Default)]
struct MobGrid {
    /// The mobs' boxes by column and the list they were read from: they hold until that
    /// list's next write ([`ListStamp`]), which every push, pick-up, death and join is.
    built: Option<(ListStamp, SeenGrid)>,
    /// The queries walked since the grid went stale, and how many are before the next grid.
    walked: u32,
    patience: u32,
}

/// `Entity.canRide` for a mob species: `Warden` and `WitherBoss` override it
/// to return false, so no vehicle ever takes them aboard.
pub(super) fn species_can_ride(species: Species) -> bool {
    !matches!(species, Species::Warden | Species::Wither)
}

/// `Entity.isVehicle`: a player or a mob rides it.
fn carries(m: &Mob) -> bool {
    m.rider.is_some() || m.extra.rider.is_some()
}
/// `isPushable`, as `pushableBy` and `canVehicleCollide` ask it:
/// `Mob::pushable` plus the shulkers it leaves out (they take no push, but
/// count).
fn pushable(m: &Mob) -> bool {
    !m.removed && (m.pushable() || m.species() == Species::Shulker && m.alive())
}
/// Whether `Entity.push` moves the mob: it is pushable and carries no one.
fn moved_by_push(m: &Mob) -> bool {
    !m.removed && m.pushable() && !carries(m)
}
/// `Entity.push(Entity)`: the push `other` gets from `this` (which takes the
/// opposite one), or None when they stand on nearly the same spot.
pub(super) fn push_impulse(this: Vec3, other: Vec3) -> Option<Vec3> {
    let (mut xa, mut za) = (other.x - this.x, other.z - this.z);
    let mut dd = xa.abs().max(za.abs());
    if dd < f64::from(0.01_f32) {
        return None;
    }
    dd = dd.sqrt();
    xa /= dd;
    za /= dd;
    let pow = (1.0 / dd).min(1.0);
    let k = f64::from(0.05_f32);
    Some(Vec3::new(xa * pow * k, 0.0, za * pow * k))
}
/// `AbstractBoat.push`: another boat is pushed while it is lower than this
/// boat's top, anything else while its feet are no higher than the boat's.
pub(super) fn boat_pushes(boat: Aabb, other: Aabb, other_is_boat: bool) -> bool {
    if other_is_boat {
        other.min_position().y < boat.max_position().y
    } else {
        other.min_position().y <= boat.min_position().y
    }
}

impl Game<'_> {
    /// Starts or ends a stretch of vehicle ticks (`tick_boats`, `tick_carts`): while one runs,
    /// a vehicle's box query ([`Self::mobs_meeting`]) reads the mobs near it from a grid of
    /// their boxes, or tests the mobs' positions against their [`BoxReach`] first. A mob
    /// joining or leaving works the reach out again, and so does [`Self::vehicle_picked_up`].
    /// The first query that needs one works it out (the reach's count is one no list has), so
    /// a level without boats or carts never walks its mobs for them.
    pub(super) fn vehicles_ticking(&self, on: bool) {
        let reach = (on && mod_api::__private::mods().is_empty())
            .then(|| (usize::MAX, BoxReach::default()));
        self.vehicle_mobs.reach.set(reach);
        // The boats' grid serves the carts ticking right after them; an older one goes.
        let mut grid = self.vehicle_mobs.grid.borrow_mut();
        let stamp = self.remote_entities.stamp();
        let built = grid.built.take().filter(|(s, _)| *s == stamp);
        *grid = MobGrid {
            built,
            ..Default::default()
        };
    }
    /// A vehicle picked a mob up: a shulker turns its lid up (`Shulker.startRiding`), which
    /// changes its box, so the next query works the [`BoxReach`] out again.
    pub(super) fn vehicle_picked_up(&self) {
        if let Some((_, reach)) = self.vehicle_mobs.reach.get() {
            self.vehicle_mobs.reach.set(Some((usize::MAX, reach)));
        }
    }
    /// The mobs' [`BoxReach`] while vehicles tick, none otherwise.
    pub(super) fn vehicle_mob_reach(&self) -> Option<BoxReach> {
        let (count, reach) = self.vehicle_mobs.reach.get()?;
        if count == self.remote_entities.len() {
            return Some(reach);
        }
        let reach = BoxReach::of(&self.remote_entities);
        self.vehicle_mobs.reach.set(Some((self.remote_entities.len(), reach)));
        Some(reach)
    }
    /// The slots, ascending, of the mobs whose vehicle is `id`, whatever their state. While
    /// vehicles tick, one walk over the mobs answers for every vehicle until the mob list is
    /// next written: each boat asks for its passengers and each cart on rails whether it is
    /// ridden, and each one walked every mob of the level.
    fn vehicle_riders(&self, id: u32) -> Vec<usize> {
        let rides = |v: u32| move |&j: &usize| self.remote_entities[j].extra.mount == Some(v);
        if !self.vehicle_mobs_indexed() {
            return (0..self.remote_entities.len()).filter(rides(id)).collect();
        }
        let stamp = self.remote_entities.stamp();
        let mut riders = self.vehicle_mobs.riders.borrow_mut();
        if riders.as_ref().is_none_or(|(s, _)| *s != stamp) {
            let mut by_vehicle = FxHashMap::<u32, Vec<usize>>::default();
            for (j, m) in self.remote_entities.iter().enumerate() {
                if let Some(v) = m.extra.mount {
                    by_vehicle.entry(v).or_default().push(j);
                }
            }
            *riders = Some((stamp, by_vehicle));
        }
        let of = riders
            .as_ref()
            .and_then(|(_, by_vehicle)| by_vehicle.get(&id));
        of.cloned().unwrap_or_default()
    }
    /// Whether a mob (one not removed) rides vehicle `id`: `Entity.isVehicle` for a cart.
    pub(in crate::game) fn mob_rides(&self, id: u32) -> bool {
        if self.ecs.is_active() && self.ecs_rider_of(id).is_some() {
            return true;
        }
        let riders = self.vehicle_riders(id);
        riders.iter().any(|&j| !self.remote_entities[j].removed)
            || self.persist.off_level.values().any(|entity| {
                matches!(entity, super::persist::OffLevel::Mob(m) if !m.removed && m.extra.mount == Some(id))
            })
    }
    /// Whether what a stretch of vehicle ticks reads from the mob list may stand in for a walk
    /// over it: a stretch runs, and no native mod is loaded (a hook can move a mob unseen, as
    /// for the carts' `CartIndex`).
    fn vehicle_mobs_indexed(&self) -> bool {
        self.vehicle_mobs.reach.get().is_some() && mod_api::__private::mods().is_empty()
    }
    /// The slots, ascending, of the mobs whose box may meet `area` into `out` (a superset: the
    /// caller still tests each box), from a grid of the mobs' boxes that the first query of a
    /// stretch of vehicle ticks builds. False: walk them all. That is outside a stretch or with
    /// native mods loaded ([`Self::vehicle_mobs_indexed`]), when the grid cannot say, and for
    /// a while after a write to the mob list ([`MobGrid`]).
    fn vehicle_mobs_near(&self, area: Aabb, out: &mut Vec<u32>) -> bool {
        if !self.vehicle_mobs_indexed() {
            return false;
        }
        let stamp = self.remote_entities.stamp();
        let mut grid = self.vehicle_mobs.grid.borrow_mut();
        if grid.built.as_ref().is_none_or(|(s, _)| *s != stamp) {
            if grid.walked < grid.patience {
                grid.walked += 1;
                return false;
            }
            grid.built = Some((stamp, SeenGrid::new(self.remote_entities.iter().map(Mob::bounds))));
            grid.walked = 0;
            grid.patience = (grid.patience * 2).max(WALKS_BEFORE_GRID);
        }
        let built = grid.built.as_ref();
        built.is_some_and(|(_, cells)| cells.candidates(area, out))
    }
    /// A vehicle's box query: `each` gets every mob whose box meets `area`, in slot order,
    /// with its slot and its box. While vehicles tick it reads the mobs near the area
    /// ([`Self::vehicle_mobs_near`]) or, walking them all, tests each position against the
    /// [`BoxReach`] before working the box out. The mobs and their order are those of a walk
    /// that tests every box.
    pub(in crate::game) fn mobs_meeting(
        &self,
        area: Aabb,
        mut each: impl FnMut(usize, &Mob, Aabb),
    ) {
        let mut near = self.vehicle_mobs.near.take();
        if self.vehicle_mobs_near(area, &mut near) {
            for &j in &near {
                let m = &self.remote_entities[j as usize];
                let b = m.bounds();
                if b.intersects(area) {
                    each(j as usize, m, b);
                }
            }
        } else {
            let reach = self.vehicle_mob_reach();
            for (j, m) in self.remote_entities.iter().enumerate() {
                if reach.is_some_and(|r| !r.may_meet(m, area)) {
                    continue;
                }
                let b = m.bounds();
                if b.intersects(area) {
                    each(j, m, b);
                }
            }
        }
        self.vehicle_mobs.near.set(near);
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
    pub(super) fn boat_mobs(&self, id: u32) -> Vec<usize> {
        let mut riders = self.vehicle_riders(id);
        riders.retain(|&j| !self.remote_entities[j].removed && self.remote_entities[j].alive());
        riders
    }
    /// `getPassengers().size()` of boat `id`.
    pub(super) fn boat_passengers(&self, id: u32) -> usize {
        self.boat_players(id)
            + self
                .mob_objects()
                .filter(|m| m.extra.mount == Some(id) && !m.removed)
                .count()
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
        let count = if self.persist.off_level.is_empty() {
            players.max(1) + self.boat_mobs(id).len()
        } else {
            self.boat_passengers(id) + usize::from(players == 0)
        };
        self.ecs.boat_seat(id, 0, count).unwrap_or(Vec3::ZERO)
    }
    /// `getRootVehicle` of mob `j`: the mob or boat at the bottom of what it
    /// rides, None when it rides nothing.
    pub(super) fn mob_root_vehicle(&self, mut j: usize) -> Option<u32> {
        let mut root = None;
        for _ in 0..8 {
            let Some(v) = self.remote_entities[j].extra.mount else {
                break;
            };
            root = Some(v);
            match self.remote_entities.iter().position(|m| m.id == v) {
                Some(k) => j = k,
                None => break,
            }
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
        self.mobs_meeting(area, |j, m, b| {
            if pushable(m) && (m.extra.mount.is_none() || self.mob_root_vehicle(j) != Some(id)) {
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
    /// What boat `b` may run into this tick.
    pub(super) fn boat_move_obstacles(&self, b: &Boat) -> Vec<Aabb> {
        let reach = b.velocity.length() + 1.0;
        self.boat_obstacles(b.id, b.bounds().inflate(Vec3::new(reach, reach, reach)))
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
            if self
                .mob_object(rider)
                .is_some_and(|mob| !mob.removed && mob.extra.mount == Some(vehicle))
            {
                self.dismount_cart_mob_at(rider, y);
            }
        }
    }

    /// `ejectPassengers` for the mobs in boat `i`: each gets out where
    /// `getDismountLocationForPassenger` puts it (beside the boat when it
    /// broke: `LivingEntity.dismountVehicle`) and can't board for a while.
    pub(super) fn eject_boat_mobs(&mut self, id: u32, broken: bool) {
        let Some(y) = self.ecs.position(id).map(|p| p[1]) else {
            return;
        };
        for rider in self.mob_passenger_ids(id) {
            if self
                .mob_object(rider)
                .is_none_or(|m| m.removed || m.extra.mount != Some(id))
            {
                continue;
            }
            if !self.native_mob_may_ride(rider, id, false) {
                continue;
            }
            let Some(m) = self
                .mob_object(rider)
                .filter(|m| !m.removed && m.extra.mount == Some(id))
            else {
                continue;
            };
            let at = if broken {
                self.removed_vehicle_dismount_point(
                    m.position,
                    f64::from(m.width()),
                    f64::from(m.height()),
                    y,
                )
            } else if let Some(boat) = self.boat_object(id) {
                self.boat_dismount_target(boat, f64::from(m.width()), m.yaw)
            } else {
                self.removed_vehicle_dismount_point(
                    m.position,
                    f64::from(m.width()),
                    f64::from(m.height()),
                    y,
                )
            };
            let m = self.mob_object_mut(rider).unwrap();
            m.extra.mount = None;
            m.extra.passenger = false;
            m.extra.boarding_cooldown = BOARDING_COOLDOWN;
            m.position = at;
            m.previous = at;
            self.snap_mob_passengers(rider);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_is_capped_and_scaled() {
        // One block apart on x: the full 0.05 along x.
        let v = push_impulse(Vec3::ZERO, Vec3::new(1.0, 0.0, 0.0)).unwrap();
        assert!((v.x - f64::from(0.05_f32)).abs() < 1e-12 && v.z == 0.0);
        // Almost on the same spot: no push.
        assert!(push_impulse(Vec3::ZERO, Vec3::new(0.005, 0.0, 0.0)).is_none());
    }

    #[test]
    fn warden_and_wither_never_board() {
        assert!(!species_can_ride(Species::Warden));
        assert!(!species_can_ride(Species::Wither));
        assert!(species_can_ride(Species::Cow));
    }

    #[test]
    #[ignore = "daily"]
    fn a_mod_growing_a_mob_during_vehicle_ticks_keeps_it_in_collision_queries() {
        use super::super::native_entity_refs::tests::with_game;

        struct Resize;
        #[mod_api::__subscriptions]
        impl mod_api::Hooks for Resize {}

        with_game(|game| {
            let kind = game.res.mob_types().get("minecraft:cow").unwrap().clone();
            let mut cow = Mob::new(77, kind, Vec3::ZERO, 0.0);
            horizon_client::mobs::finalize::set_baby(&mut cow, true);
            game.remote_entities.push(cow);
            let mob = game.mob(game.entity(77).unwrap()).unwrap();
            let area = Aabb::new(Vec3::new(0.3, 0.0, 0.0), Vec3::new(0.4, 1.0, 0.1));
            assert!(!game.remote_entities[0].bounds().intersects(area));
            let mods = vec![mod_api::ModEntry::new("resize-test", Box::new(Resize))].into();
            mod_platform::testing::with_mods(mods, || {
                game.vehicles_ticking(true);
                assert!(mob.set_baby(game, false));
                let cow = &game.remote_entities[0];
                assert!(cow.bounds().intersects(area));
                assert!(
                    game.vehicle_mob_reach()
                        .is_none_or(|reach| reach.may_meet(cow, area))
                );
                game.vehicles_ticking(false);
            });
        });
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

    /// A vehicle's box query during a stretch of vehicle ticks names the mobs a walk over
    /// every box names, in the same order, also right after a mob moved into the box (a
    /// push or a pick-up writes the mob list in the middle of a stretch).
    #[test]
    #[ignore = "daily"]
    fn a_vehicles_box_query_matches_a_walk_over_every_mob() {
        let mut resources = None;
        let mut game = super::super::game_tests::tests::server_game(&mut resources);
        let pig = game.res.mob_types().get("minecraft:pig").unwrap().clone();
        // A small LCG keeps the scene the same on every run.
        let mut seed = 0x2545_f491_4f6c_dd1du64;
        let mut next = |range: f64| {
            seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            (seed >> 11) as f64 / (1u64 << 53) as f64 * range
        };
        for id in 1..=400 {
            let at = Vec3::new(next(48.0), 64.0 + next(3.0), next(48.0));
            game.remote_entities.push(Mob::new(id, pig.clone(), at, 0.0));
        }
        let mut met = 0;
        for round in 0..300 {
            // Three stretches, each with two writes to the mob list.
            if round % 100 == 0 {
                game.vehicles_ticking(true);
            }
            let corner = Vec3::new(next(46.0), 63.0 + next(3.0), next(46.0));
            let size = Vec3::new(0.5 + next(2.5), 0.5 + next(1.5), 0.5 + next(2.5));
            let area = Aabb::new(corner, corner.add_vector(size));
            if round % 40 == 20 {
                let j = round % game.remote_entities.len();
                game.remote_entities[j].position = area.center();
            }
            let walked: Vec<Aabb> = game
                .remote_entities
                .iter()
                .map(Mob::bounds)
                .filter(|b| b.intersects(area))
                .collect();
            met += walked.len();
            assert_eq!(game.boat_obstacles(u32::MAX, area), walked, "round {round}");
            if round % 100 == 99 {
                // The last comparisons ran against the grid: it stands, for the list as it is.
                let fresh = |(stamp, _): &(ListStamp, SeenGrid)| *stamp == game.remote_entities.stamp();
                assert!(
                    game.vehicle_mobs
                        .grid
                        .borrow()
                        .built
                        .as_ref()
                        .is_some_and(fresh)
                );
                game.vehicles_ticking(false);
            }
        }
        assert!(met > 100, "the boxes met {met} mobs");
    }

    /// Who rides a vehicle follows the mob list through a stretch of vehicle ticks: a mob that
    /// boards counts from then on, one removed no longer does, and a dead one still makes
    /// its cart a vehicle but is no passenger of a boat.
    #[test]
    #[ignore = "daily"]
    fn a_vehicle_knows_its_mob_riders_as_soon_as_they_board() {
        let mut resources = None;
        let mut game = super::super::game_tests::tests::server_game(&mut resources);
        let pig = game.res.mob_types().get("minecraft:pig").unwrap().clone();
        for id in 1..=4 {
            game.remote_entities.push(Mob::new(id, pig.clone(), Vec3::ZERO, 0.0));
        }
        let riders =
            |game: &Game| [100, 101, 102].map(|id| (game.mob_rides(id), game.boat_mobs(id)));
        game.remote_entities[0].extra.mount = Some(100);
        game.vehicles_ticking(true);
        let none = (false, vec![]);
        assert_eq!(riders(&game), [(true, vec![0]), none.clone(), none.clone()]);
        game.remote_entities[0].removed = true;
        game.remote_entities[1].extra.mount = Some(101);
        game.remote_entities[3].extra.mount = Some(102);
        game.remote_entities[3].health = 0.0;
        let after = [none, (true, vec![1]), (true, vec![])];
        assert_eq!(riders(&game), after);
        game.vehicles_ticking(false);
        assert_eq!(riders(&game), after);
    }
}

#[cfg(test)]
mod off_level_tests {
    use super::*;

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
