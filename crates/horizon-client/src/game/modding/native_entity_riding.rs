//! Vehicles and passengers for native server mods, through an `EntityRef`
//! (docs/modding/src/how-to/native-mods.md, "Entity and level handles", #7387): what an entity
//! rides, who rides it, getting off, and who holds its lead. Players ride
//! boats, minecarts and mounts; mobs ride mobs, boats and minecarts. Reads
//! return `None` for a stale handle or on a client.
use super::{Game, commands::Target, mounts::steering_item};
use horizon_client::{
    mobs::{Mob, Species, equine, happy_ghast, leash::Holder, nautilus},
};
use mod_api::{EntityRef, LeashHolder, LivingRef};

impl Game<'_> {
    /// The dismount search with a vehicle or passenger lent out of the mob list.
    pub(super) fn scoped_mob_dismount_location(
        &self,
        vehicle: Option<&Mob>,
        vehicle_id: u32,
        rider: Option<&Mob>,
        passenger: EntityRef,
    ) -> Option<[f64; 3]> {
        let at = if let Some(rider) = rider {
            if self.is_boat_id(vehicle_id) {
                self.boat_dismount_target(vehicle_id, f64::from(rider.width()), rider.yaw)
            } else if let Some(mount) = vehicle {
                Some(
                    self.horse_dismount_mobs(mount, rider)
                        .unwrap_or_else(|| super::mounts::on_top(mount)),
                )
            } else {
                self.cart_dismount_mob(vehicle_id, rider)
            }
        } else if let Some(mount) = vehicle {
            match self.native_entity_target(passenger)? {
                Target::Player => {
                    Some(self.player_dismount_from_lent_mob(mount, self.yaw, self.player.as_ref()?))
                }
                Target::Guest(conn) => {
                    let guest = self.command_guest(conn)?;
                    Some(self.player_dismount_from_lent_mob(
                        mount,
                        guest.yaw,
                        guest.motion.as_deref()?,
                    ))
                }
                _ => None,
            }
        } else {
            let vehicle = self.entity(vehicle_id)?;
            return self.entity_dismount_location(vehicle, LivingRef::new(passenger));
        };
        if let Some(at) = at {
            return Some([at.x, at.y, at.z]);
        }
        let vehicle = self.entity(vehicle_id)?;
        let [x, _, z] = self.entity_position(vehicle)?;
        let [_, [_, top, _]] = self.entity_bounding_box(vehicle)?;
        Some([x, top, z])
    }

    /// The id of the entity `t` rides, if it rides one.
    fn native_vehicle_id(&self, t: Target) -> Option<u32> {
        match t {
            Target::Player => self.riding,
            Target::Guest(conn) => self.command_guest(conn)?.vehicle,
            Target::Mob(id) if self.ecs.exists(id) => self.ecs_vehicle_of(id),
            Target::Mob(_) => None,
            // A general row rides the row its tree loaded with; a row queued
            // for removal rides nothing, as a removed mob does not.
            Target::General(id) => self
                .resident_entities
                .general
                .rows
                .iter()
                .find(|row| row.id == id && !self.persist.removals.contains(&Target::General(id)))
                .and_then(|row| row.mount),
            _ => None,
        }
    }
    /// The entity's vehicle as a handle, if it rides one in this level.
    fn native_vehicle(&self, entity: EntityRef) -> Option<Option<EntityRef>> {
        let t = self.native_entity_target(entity)?;
        Some(self.native_vehicle_id(t).and_then(|v| self.entity(v)))
    }
    /// What the entity rides (`Entity.getVehicle`): `None` when it rides
    /// nothing, its actual vehicle is off-level, or the input handle is stale.
    pub(super) fn entity_vehicle(&self, entity: EntityRef) -> Option<EntityRef> {
        self.native_vehicle(entity).flatten()
    }
    /// Whether the entity rides another object, including a retained off-level vehicle.
    pub(super) fn entity_is_passenger(&self, entity: EntityRef) -> Option<bool> {
        Some(
            self.native_vehicle_id(self.native_entity_target(entity)?)
                .is_some(),
        )
    }
    /// The lowest vehicle under the entity (`Entity.getRootVehicle`): the
    /// entity itself when it rides nothing. Off-level intermediaries are traversed;
    /// an off-level root or a cycle returns no handle, never a descendant substitute.
    pub(super) fn entity_root_vehicle(&self, entity: EntityRef) -> Option<EntityRef> {
        let target = self.native_entity_target(entity)?;
        self.entity(self.native_root_vehicle_id(entity.id(), target)?)
    }

    /// Follow full object links, returning no root for a cycle. Only mobs can
    /// be intermediate vehicles. Their object count plus the non-Mob endpoints
    /// bounds the walk without truncating valid trees or allocating a visited set.
    pub(super) fn native_root_vehicle_id(&self, id: u32, target: Target) -> Option<u32> {
        let mut root = id;
        let mut next = self.native_vehicle_id(target);
        for _ in 0..=self.ecs.entity_count() + 1 {
            let Some(vehicle) = next else {
                return Some(root);
            };
            root = vehicle;
            next = self.native_vehicle_id(Target::Mob(vehicle));
        }
        None
    }
    /// A live portal root carries a direct or indirect mount through complete objects.
    pub(super) fn vehicle_tree_contains_mount(&self, root: u32, mount: u32) -> bool {
        self.entity(root).is_some()
            && self.entity(mount).is_some()
            && self.native_root_vehicle_id(mount, Target::Mob(mount)) == Some(root)
    }

    /// The entities riding this one (`Entity.getPassengers`), in `addPassenger`'s order.
    pub(super) fn entity_passengers(&self, entity: EntityRef) -> Option<Vec<EntityRef>> {
        self.native_entity_target(entity)?;
        let seats = self.seated_passengers(entity.id()).into_iter();
        Some(seats.filter_map(|(_, id, _)| self.entity(id)).collect())
    }
    /// Whether any object rides the entity, including retained off-level passengers.
    pub(super) fn entity_is_vehicle(&self, entity: EntityRef) -> Option<bool> {
        self.native_entity_target(entity)?;
        Some(!self.seated_passengers(entity.id()).is_empty())
    }
    /// Every entity riding this one, at any depth (`Entity.getIndirectPassengers`): each
    /// passenger in boarding order, followed by those riding it. Traverse off-level
    /// intermediaries but return only resident handles.
    pub(super) fn entity_indirect_passengers(&self, entity: EntityRef) -> Option<Vec<EntityRef>> {
        self.native_entity_target(entity)?;
        let mut all: Vec<EntityRef> = Vec::new();
        // This vector allocates only when a retained, unrepresentable object is
        // actually encountered; ordinary resident-only trees keep two buffers.
        let mut visited_absent = Vec::new();
        let mut next: Vec<u32> = self
            .seated_passengers(entity.id())
            .into_iter()
            .rev()
            .map(|(_, id, _)| id)
            .collect();
        while let Some(passenger) = next.pop() {
            if passenger == entity.id()
                || all.iter().any(|r| r.id() == passenger)
                || visited_absent.contains(&passenger)
            {
                continue;
            }
            if let Some(handle) = self.entity(passenger) {
                all.push(handle);
            } else {
                visited_absent.push(passenger);
            }
            next.extend(
                self.seated_passengers(passenger)
                    .into_iter()
                    .rev()
                    .map(|(_, id, _)| id),
            );
        }
        Some(all)
    }
    /// Whether the player `t` holds the stick that steers `species` in either hand.
    fn native_player_steers(&self, species: Species, t: Target) -> bool {
        let (main, offhand) = match t {
            Target::Player => (self.held(), self.offhand.as_ref()),
            Target::Guest(conn) => {
                let Some(guest) = self.command_guest(conn) else {
                    return false;
                };
                (
                    guest.inventory.get(guest.selected).and_then(Option::as_ref),
                    guest.offhand.as_ref(),
                )
            }
            _ => return false,
        };
        steering_item(
            species,
            main.map(|s| s.item.as_ref()),
            offhand.map(|s| s.item.as_ref()),
        )
    }
    /// The overrides of `getControllingPassenger` on `mob`: whether the player `rider`, its
    /// first passenger, steers it.
    pub(super) fn native_player_controls(&self, mob: &Mob, rider: Target) -> bool {
        let species = mob.species();
        match species {
            Species::Pig | Species::Strider => {
                mob.extra.saddled() && self.native_player_steers(species, rider)
            }
            Species::HappyGhast => {
                happy_ghast::wearing_harness(mob) && !happy_ghast::on_still_timeout(mob)
            }
            s if equine::is_equine(s) || nautilus::is_nautilus(s) => mob.extra.saddled(),
            _ => false,
        }
    }
    /// `native_player_controls` for the ECS mob `id`: the saddle is the mount's saddle slot.
    /// A happy ghast's harness and its still timeout have no ECS reader yet (RESTE-SANS-ECS: never steered).
    fn native_ecs_player_controls(&self, id: u32, rider: Target) -> bool {
        let Some(species) = self.creature_species(id) else {
            return false;
        };
        let saddled = self.ecs.mount_contents(id).is_some_and(|(saddle, _, _)| saddle.is_some());
        match species {
            Species::Pig | Species::Strider => saddled && self.native_player_steers(species, rider),
            s if equine::is_equine(s) || nautilus::is_nautilus(s) => saddled,
            _ => false,
        }
    }
    /// The first passenger, when it steers the entity (`Entity.getControllingPassenger`): a
    /// player in a boat, a player on a mount that is ready for one, or a mob on a mob with AI
    /// unless `#non_controlling_rider` names it. `None` when nothing steers it, or for a stale
    /// handle.
    pub(super) fn entity_controlling_passenger(&self, entity: EntityRef) -> Option<LivingRef> {
        let vehicle = self.native_entity_target(entity)?;
        let first_id = self.seated_passengers(entity.id()).first()?.1;
        let first = self.entity(first_id)?;
        let rider = self.native_entity_target(first)?;
        let is_player = matches!(rider, Target::Player | Target::Guest(_));
        match vehicle {
            Target::Boat(_) if is_player => Some(LivingRef::new(first)),
            Target::Mob(id) => {
                if is_player {
                    return self
                        .native_ecs_player_controls(id, rider)
                        .then(|| LivingRef::new(first));
                }
                let kind = self.ecs.kind(first.id())?;
                let tagged = self.res.mob_data().entity_has("minecraft:non_controlling_rider", kind);
                let no_ai = self.ecs_flag(id, "NoAI").unwrap_or(false);
                (!no_ai && !tagged).then(|| LivingRef::new(first))
            }
            _ => None,
        }
    }
    /// Whether both entities share a root vehicle (`Entity.isPassengerOfSameVehicle`): true for
    /// an entity and itself, false for two entities riding nothing. `None` for a stale handle.
    pub(super) fn entity_is_passenger_of_same_vehicle(
        &self,
        entity: EntityRef,
        other: EntityRef,
    ) -> Option<bool> {
        let target = self.native_entity_target(entity)?;
        let other_target = self.native_entity_target(other)?;
        Some(
            self.native_root_vehicle_id(entity.id(), target)?
                == self.native_root_vehicle_id(other.id(), other_target)?,
        )
    }
    /// Where getting off this vehicle puts `passenger` as it stands now, riding it or not. This
    /// is the game's own placement, from the code `stop_riding` runs, so it can differ from
    /// `Entity.getDismountLocationForPassenger`. A player leaves a boat ahead of them or on top
    /// of it, a cart beside or on top of it, a strider by vanilla's search, a happy ghast on
    /// top, and any other mob beside or on top of it. A mob leaves a boat as a player does, a
    /// cart by vanilla's search, a horse, camel or llama at its side or feet, and any other mob
    /// on top: vanilla's searches for `Animal` and the strider don't reach mobs yet, and those
    /// for horses, `Animal` and the cart don't reach players. Nothing rides anything else, so
    /// there it is the top of the box. Reads only: no `mob_ride` or dismount veto is asked and
    /// the passenger's pose stays. `None` for a stale handle, a passenger other than a player
    /// or a mob, a player with no motion state, or a vehicle whose box Horizon does not keep.
    pub(super) fn entity_dismount_location(
        &self,
        vehicle: EntityRef,
        passenger: LivingRef,
    ) -> Option<[f64; 3]> {
        self.native_entity_target(vehicle)?;
        let id = vehicle.id();
        let at = match self.native_entity_target(passenger.entity())? {
            Target::Mob(rider) => self.mob_dismount_point(id, rider),
            Target::Player => self.player_dismount_point(id, self.yaw, self.player.as_ref()?),
            Target::Guest(conn) => {
                let guest = self.command_guest(conn)?;
                self.player_dismount_point(id, guest.yaw, guest.motion.as_deref()?)
            }
            _ => return None,
        };
        if let Some(at) = at {
            return Some([at.x, at.y, at.z]);
        }
        let [x, _, z] = self.entity_position(vehicle)?;
        let [_, [_, top, _]] = self.entity_bounding_box(vehicle)?;
        Some([x, top, z])
    }
    /// The live holder of a mob's or boat's lead (`Leashable.getLeashHolder`), or `Unleashed`
    /// when no lead exists. `None` means an entity that takes no lead, or that the entity,
    /// holder or saved holder is unavailable, stale or in a parked level. Does not load a level.
    pub(super) fn entity_leash_holder(&self, entity: EntityRef) -> Option<LeashHolder> {
        match self.native_entity_target(entity)? {
            Target::Mob(id) | Target::Boat(id) => self.ecs_leash_holder_ref(id),
            _ => None,
        }
    }
    /// The live holder of the lead on the ECS entity `id` as a handle: `Unleashed` with no lead; `None` when the entity or the
    /// holder is not in the level.
    fn ecs_leash_holder_ref(&self, id: u32) -> Option<LeashHolder> {
        use horizon_ecs_proto::leash::Holder as EcsHolder;
        self.ecs.entity_of(id)?;
        Some(match self.ecs_leash_holder(id) {
            None => LeashHolder::Unleashed,
            Some(EcsHolder::Knot(pos)) => LeashHolder::Knot(pos),
            Some(EcsHolder::Player(player)) => LeashHolder::Entity(self.entity(player)?),
            Some(EcsHolder::Entity(e)) => {
                let held = horizon_ecs_proto::bridge::ids::id_of(self.ecs.world()?, e)?;
                LeashHolder::Entity(self.entity(held)?)
            }
        })
    }
    /// Gives the holder the mob's or boat's lead (`Leashable.setLeashedTo`): the entity lets go
    /// of any other holder and gets off its vehicle, where a mod's `mob_ride` lets it. As
    /// there, neither `canBeLeashed` nor the distance is asked: a holder too far away snaps
    /// the lead on the next tick. False for a stale handle, the entity itself, a holder other
    /// than a player, mob or boat, or an entity that takes no lead.
    pub(super) fn entity_leash_to(&mut self, entity: EntityRef, holder: EntityRef) -> bool {
        let (Some(target), Some(by)) = (
            self.native_entity_target(entity),
            self.native_entity_target(holder),
        ) else {
            return false;
        };
        if entity == holder {
            return false;
        }
        let (holder, player) = match by {
            Target::Mob(id) => (Holder::Mob(id), None),
            Target::Boat(id) => (Holder::Boat(id), None),
            Target::Player | Target::Guest(_) => match self.native_player_facts(by) {
                Some(facts) => (Holder::Player, Some(facts.uuid)),
                None => return false,
            },
            _ => return false,
        };
        self.native_set_leash(entity, target, holder, player)
    }
    /// Ties the mob's or boat's lead to the fence at `pos`, as `entity_leash_to` does to
    /// an entity; the knot is there while a lead hangs on it. False for a stale handle, an
    /// entity that takes no lead, or no fence at `pos`.
    pub(super) fn entity_leash_to_knot(&mut self, entity: EntityRef, pos: [i32; 3]) -> bool {
        let Some(target) = self.native_entity_target(entity) else {
            return false;
        };
        let block = self
            .res
            .facts()
            .get(self.world.block(pos[0], pos[1], pos[2]));
        if !self.named_block_in_tag("minecraft:fences", &block.name) {
            return false;
        }
        self.native_set_leash(entity, target, Holder::Knot(pos), None)
    }
    /// `Leashable.setLeashedTo` from a handle: the lead first, then off the vehicle.
    fn native_set_leash(
        &mut self,
        entity: EntityRef,
        target: Target,
        holder: Holder,
        player: Option<[i32; 4]>,
    ) -> bool {
        let leashed = self.set_leash(target, holder, player);
        if leashed {
            self.entity_stop_riding(entity);
        }
        leashed
    }
    /// Takes the lead off a mob or boat: the lead falls as an item with `drop_lead`
    /// (`Leashable.dropLeash`), and is gone without it (`removeLeash`). A mob's removal is heard
    /// by `entity_unleash`. False when no holder has the lead, for a stale handle, or for an
    /// entity that takes no lead. A holder still unresolved from a save is left alone.
    pub(super) fn entity_detach_leash(&mut self, entity: EntityRef, drop_lead: bool) -> bool {
        self.native_entity_target(entity)
            .is_some_and(|target| self.detach_leash(target, drop_lead))
    }
    /// Attempts one mount, shared by handle writes and the ride command.
    pub(super) fn start_riding_target(
        &mut self,
        rider: Target,
        vehicle: Target,
        force: bool,
    ) -> bool {
        if let Target::Guest(conn) = rider {
            if self
                .command_guest(conn)
                .is_none_or(|guest| guest.dimension != self.dim.id())
            {
                return false;
            }
            return self
                .for_player(rider, |game| {
                    game.start_riding_target(Target::Player, vehicle, force)
                })
                .unwrap_or(false);
        }
        let Some(vehicle_id) = self.native_riding_preconditions(rider, vehicle, force) else {
            return false;
        };
        let previous_vehicle = self.native_vehicle_id(rider);
        let allowed = match rider {
            Target::Player => self.native_may_board(vehicle_id),
            Target::Mob(id) => self.native_mob_may_ride(id, vehicle_id, true),
            _ => false,
        };
        if !allowed
            || self.native_vehicle_id(rider) != previous_vehicle
            || self
                .native_riding_preconditions(rider, vehicle, force)
                .is_none()
        {
            return false;
        }
        if previous_vehicle.is_some() {
            match rider {
                Target::Player => self.stop_riding(),
                Target::Mob(id) if self.ecs.exists(id) => self.ecs_dismount(id),
                Target::Mob(id) => {
                    if previous_vehicle.is_some_and(|v| {
                        self.persist.mount_notifications.contains(&(id, v, false))
                            && self.persist.removals.iter().any(|target| {
                                matches!(*target, Target::Boat(n) | Target::Cart(n) if n == v)
                            })
                    }) {
                        // This explicit callback mutation completes the in-flight edge,
                        // rather than recursively notifying it a second time.
                        self.dismount_mob_reentrant(id);
                    } else {
                        self.dismount_mob(id);
                    }
                }
                _ => return false,
            }
            if self.native_vehicle_id(rider).is_some() {
                return false;
            }
        }
        // Dismount dispatch can also discard, remount or fill the target vehicle.
        if self
            .native_riding_preconditions(rider, vehicle, force)
            .is_none()
        {
            return false;
        }
        match rider {
            Target::Player => {
                match vehicle {
                    // `ServerPlayer.startRiding` teleports with `Relative.ROTATION`: the
                    // rider keeps their view, whatever the vehicle's yaw.
                    Target::Boat(_) => {}
                    Target::Mob(id) => {
                        self.ecs.clear_jump_charge(id);
                        self.mount.jump_ticks = 0;
                        self.mount.jump_scale = 0.0;
                    }
                    Target::Stand(id) | Target::Other(id) => {
                        let Some(seat) = self.seat(id) else {
                            return false;
                        };
                        self.board_seat(id, seat);
                        return self.native_vehicle_id(rider) == Some(vehicle_id);
                    }
                    _ => {}
                }
                self.riding = Some(vehicle_id);
                self.seat_acting_player(vehicle_id);
                self.trigger_player("minecraft:started_riding");
            }
            Target::Mob(id) if self.ecs.exists(id) => {
                if !self.ecs_mount(id, vehicle_id) {
                    return false;
                }
                // Gap: the ECS places the rider, so `seat_mob` (host list) is not called here.
                self.break_leash(id, true, false);
            }
            _ => return false,
        }
        self.native_vehicle_id(rider) == Some(vehicle_id)
    }

    /// Validate the captured relationship at each callback boundary.
    fn native_riding_preconditions(
        &self,
        rider: Target,
        vehicle: Target,
        force: bool,
    ) -> Option<u32> {
        let vehicle_id = match vehicle {
            Target::Mob(id)
            | Target::Boat(id)
            | Target::Cart(id)
            | Target::Stand(id)
            | Target::Other(id) => id,
            _ => return None,
        };
        if (self.entity(vehicle_id).is_none() && !self.ecs.exists(vehicle_id))
            || matches!(rider, Target::Mob(id) if !self.ecs.exists(id))
            || !matches!(rider, Target::Player | Target::Mob(_))
            || rider == vehicle
            || self.native_vehicle_id(rider) == Some(vehicle_id)
        {
            return None;
        }
        // Intermediate vehicles can be retained objects, not ordinary handles.
        let mut current = Some(vehicle_id);
        // The ECS chains are bounded by their own limit (64), beside the host's lists.
        for _ in 0..=64 {
            let Some(id) = current else { break };
            if rider == Target::Mob(id) {
                return None;
            }
            current = self.native_vehicle_id(Target::Mob(id));
        }
        if current.is_some() {
            return None;
        }
        if !force {
            match rider {
                // `Entity.canRide`: a shifting player is not boarded.
                Target::Player if self.player.as_ref().is_some_and(|p| p.shift_key_down) => {
                    return None;
                }
                Target::Mob(id)
                    if self.ecs.exists(id)
                        && !self.ecs.entity_of(id).is_some_and(|e| self.ecs.boarding_ready(e)) =>
                {
                    return None;
                }
                _ => {}
            }
        }
        match vehicle {
            Target::Boat(id) => {
                let max = self.ecs.boat_max_passengers(id)?;
                if self.boat_passengers(id) >= max
                    || (!force && !self.boat_has_room(id))
                {
                    return None;
                }
            }
            Target::Cart(id) => {
                if self.ecs.cart_kind(id).is_none()
                    || self.riding == Some(id)
                    || self.ridden_by_guest(id)
                    || !self.seated_passengers(id).is_empty()
                {
                    return None;
                }
            }
            Target::Mob(id) if self.ecs.exists(id) => {
                // A free seat is the ECS's: `ecs_mount` refuses a full vehicle, and `force` skips the check.
                if !force && !self.ecs.has_free_seat(id) {
                    return None;
                }
            }
            Target::Stand(id) | Target::Other(id) => {
                if self.seat(id).is_none() || (!force && !self.seated_passengers(id).is_empty()) {
                    return None;
                }
            }
            _ => return None,
        }
        Some(vehicle_id)
    }

    /// Attempts to board a vehicle through the shared server mount path.
    ///
    /// Players and mobs can ride a mob, boat, minecart or generic seat entity. Returns false for
    /// unsupported entity pairs, stale or held-out handles, a full stored
    /// vehicle, a cycle, or a canceled mount or dismount. Normal mob capacity
    /// follows its species and is bypassed by force; minecarts store one passenger and boats store one
    /// mob and up to two passengers in total. Generic seats admit one passenger unless forced.
    /// `force` bypasses normal admission checks, not these storage limits.
    /// Player boarding cooldown and the strider's
    /// eye-in-lava admission check are not yet represented by the game.
    pub(super) fn entity_start_riding(
        &mut self,
        entity: EntityRef,
        vehicle: EntityRef,
        force: bool,
    ) -> bool {
        let Some(rider) = self.native_entity_target(entity) else {
            return false;
        };
        let Some(vehicle) = self.native_entity_target(vehicle) else {
            return false;
        };
        self.start_riding_target(rider, vehicle, force)
    }

    /// Gets the entity off its vehicle (`Entity.stopRiding`), where the
    /// vehicle lets it out, as `/ride ... dismount` does. False when it rode
    /// nothing, a mod's `mob_ride` hook kept it on, or for a stale handle.
    pub(super) fn entity_stop_riding(&mut self, entity: EntityRef) -> bool {
        let Some(t) = self.native_entity_target(entity) else {
            return false;
        };
        let Some(vehicle) = self.native_vehicle_id(t) else {
            return false;
        };
        match t {
            Target::Mob(id) => self.dismount_mob(id),
            Target::Player | Target::Guest(_) => {
                self.for_player(t, |g| g.stop_riding());
            }
            // A general row gets off by its mount link: the row keeps its
            // identity and place, and the level's next walk sees it as a
            // root (`Entity.stopRiding` clears the mount and nothing else).
            // The mount rewrite also retires the rows' rider index, which
            // the list's own write stamp does not see.
            Target::General(id) => {
                let Some(row) = self
                    .resident_entities
                    .general
                    .rows
                    .iter_mut()
                    .find(|row| row.id == id)
                else {
                    return false;
                };
                row.mount = None;
                self.resident_entities.general.mount_writes += 1;
            }
            _ => return false,
        }
        self.native_vehicle_id(t).is_none()
    }
    /// Gets every passenger off the entity (`Entity.ejectPassengers`), last
    /// first as vanilla walks the list, so `mob_ride` hears them in its
    /// order. False when nothing rode it, or for a stale handle.
    pub(super) fn entity_eject_passengers(&mut self, entity: EntityRef) -> bool {
        let Some(passengers) = self.entity_passengers(entity) else {
            return false;
        };
        let mut any = false;
        for p in passengers.into_iter().rev() {
            any |= self.entity_stop_riding(p);
        }
        any
    }
}

#[cfg(test)]
mod tests {
    use horizon_core::geometry::Vec3;

    #[test]
    #[ignore = "daily"]
    fn boat_and_cart_interactions_use_shared_mount_path() {
        use super::super::{
            InteractTarget,
        };
        use horizon_client::motion::PlayerMotion;
        super::super::native_entity_refs::tests::with_game(|game| {
            game.player = Some(PlayerMotion::new(game.motion, Vec3::ZERO).unwrap());
            game.yaw = 10.0;
            let boat = game.spawn_boat_ecs("oak", false, [0.0; 3], 45.0).unwrap();
            assert!(game.interact_boat(Some(InteractTarget {
                id: boat as i32,
                location: [0.0; 3]
            })));
            assert_eq!(game.riding, Some(boat));
            assert_eq!(game.yaw, 10.0, "mounting keeps the rider's view");
            game.riding = None;
            assert!(game.ecs_with(|ecs, mw| ecs.spawn_cart_id(mw, "minecraft:minecart", [0.0; 3], 0.0, &mut || 61)).is_some());
            assert!(game.interact_cart(Some(InteractTarget {
                id: 61,
                location: [0.0; 3]
            })));
            assert_eq!(game.riding, Some(61));
        });
    }

    #[test]
    #[ignore = "daily"]
    fn a_rider_boards_another_cart_or_boat_straight_from_its_vehicle() {
        use super::super::{
            InteractTarget,
        };
        use horizon_client::motion::PlayerMotion;
        super::super::native_entity_refs::tests::with_game(|game| {
            game.player = Some(PlayerMotion::new(game.motion, Vec3::ZERO).unwrap());
            assert!(game.ecs_with(|ecs, mw| ecs.spawn_cart_id(mw, "minecraft:minecart", [0.0; 3], 0.0, &mut || 61)).is_some());
            assert!(game.ecs_with(|ecs, mw| ecs.spawn_cart_id(mw, "minecraft:minecart", [0.0; 3], 0.0, &mut || 62)).is_some());
            let chest_boat = game.spawn_boat_ecs("oak", true, [0.0; 3], 0.0).unwrap();
            let at = |id| {
                Some(InteractTarget {
                    id,
                    location: [0.0; 3],
                })
            };
            assert!(game.interact_cart(at(61)));
            assert_eq!(game.riding, Some(61));
            // Minecart.interact -> Entity.startRiding, which leaves the old
            // vehicle first: the rider changes carts in one click.
            assert!(game.interact_cart(at(62)));
            assert_eq!(game.riding, Some(62));
            // AbstractChestBoat.interact tries AbstractBoat.interact's
            // boarding first; the chest only opens when that passes.
            assert!(game.interact_boat(at(chest_boat as i32)));
            assert_eq!(game.riding, Some(chest_boat));
            assert_eq!(game.container, None, "the chest stays closed");
            assert!(game.interact_cart(at(61)));
            assert_eq!(game.riding, Some(61), "and from a boat into a cart");
        });
    }
}
