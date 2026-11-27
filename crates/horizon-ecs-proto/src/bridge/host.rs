//! The ECS world of one host (a dimension of a server): what the host calls (spawn, hurt, interact, chunks, NBT) and what the schedule
//! answers (`HostEvent`, read at the next `step`). The host's blocks are read through its mob world, lent to the schedule for one call.
pub use crate::bridge::players::{PlayerFact, remove_player, set_player};
pub use crate::bridge::select::Select;
use crate::behaviours::Interaction;
use crate::bridge::{
    assemble,
    events::drain_typed,
    ids::{self, NetIds},
    players, select, view,
};
use crate::cow::{Blocks, Cause, Clock, DamageQueue, Hit, Level, Mw, Pos, cause_of_damage_type, host_level, spawn_armed, species_table};
use crate::equipment_drop::death_drops;
pub use super::host_read_living::Flag;
use crate::goals::{self, Equipment, Events, Online, Player, Slot};
use crate::host_out::HostEvent;
use crate::mount::{self, Riding};
use crate::nbt::{self, Saved};
use crate::targeting::Who;
use crate::zone;
use bevy_ecs::{entity::Entity, entity_disabling::Disabled, prelude::*, query::Allow};
use myriade_engine_lib::mobs::{MobTypes, MobWorld};
use horizon_core::{geometry::{Aabb, Vec3}, nbt::Compound, random::LegacyRandom};
use std::{
    collections::HashMap,
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
};

/// What a host world starts with: the seed of its spawns, the dimension's name, and the simulation distance in chunks (0: none).
#[derive(Clone, Debug)]
pub struct HostSetup {
    pub seed: i64,
    pub dimension: &'static str,
    pub simulation_radius: i32,
}

/// The name of the dimension the world is in: the `dimension` of a `GlobalPos` (a brute's home).
#[derive(Resource, Clone, Copy, Debug)]
pub struct WorldDimension(pub &'static str);

/// Who or what hurt an entity, as the host names it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HurtSource {
    Generic,
    /// A hurt of a named damage type (`/damage`): the direct entity (`by`, a hit by an entity when set) and where it came from.
    Typed { damage: &'static str, by: Option<Who>, from: Option<(f64, f64)> },
    Player(u32),
    /// A network id of an entity (a creature, a projectile's owner).
    Entity(u32),
    Fall,
    Fire,
    Explosion,
    /// Magic that an entity's projectile or effect cast (`indirectMagic`): the owner, if any, is the last attacker, and no direct hit.
    IndirectMagic { owner: Option<Who> },
    Void,
}

pub struct EcsWorld {
    world: World,
    schedule: Schedule,
    occ: &'static crate::occupancy::Occupancy,
    spawned: u64,
    /// The NBT of the chunks that unloaded, read back by `chunk_loaded`.
    chunks: HashMap<(i32, i32), Vec<Compound>>,
}

/// The mob world lent for one call. Its lifetime is erased here: the level that holds it is taken out of the world before the call returns.
pub(super) fn lend(mw: &MobWorld<'_>) -> &'static MobWorld<'static> {
    // SAFETY: the reference lives only in the level that `lent` inserts and removes again, panics included.
    unsafe { &*(mw as *const MobWorld<'_> as *const MobWorld<'static>) }
}

fn chunk_of(v: Vec3) -> (i32, i32) {
    ((v.x.floor() as i32) >> 4, (v.z.floor() as i32) >> 4)
}

/// A command on an entity that a command before it removed (a death and a hit in one batch) does nothing, as vanilla's calls on a removed
/// entity do; every other error keeps bevy's severity (a panic for a real mistake).
/// The clock read once when the world is made: the entity seeds of one run start where no other run's do (vanilla's `RandomSupport.generateUniqueSeed`
/// mixes `System.nanoTime`), the cost is one read per world and none per entity.
fn start_clock() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64)
}

fn despawned_is_a_no_op(err: bevy_ecs::error::BevyError, ctx: bevy_ecs::error::ErrorContext) {
    if err.downcast_ref::<bevy_ecs::entity::EntityNotSpawnedError>().is_some() {
        return;
    }
    bevy_ecs::error::match_severity(err, ctx);
}

impl EcsWorld {
    /// `tags` is the resources' folder: the entity type tags are read from it, against the species table.
    pub fn new(setup: &HostSetup, mw: &MobWorld<'_>, types: &'static MobTypes, tags: &std::path::Path) -> EcsWorld {
        let level = host_level(lend(mw), types);
        let occ = level.occ;
        crate::type_tags::install(tags, &species_table().0);
        let mut world = assemble::assemble(level);
        world.remove_resource::<Level>();
        zone::enable(&mut world, setup.simulation_radius);
        world.init_resource::<view::Modified>();
        world.init_resource::<crate::queries::Queries>();
        world.init_resource::<view::Entered>();
        world.insert_resource(bevy_ecs::error::FallbackErrorHandler(despawned_is_a_no_op));
        world.insert_resource(crate::frame_item::DropRng(crate::flat::Rng(setup.seed as u64 ^ 0x9E37_79B9_7F4A_7C15)));
        world.insert_resource(WorldDimension(setup.dimension));
        EcsWorld { world, schedule: crate::cow::schedule(), occ, spawned: (setup.seed as u64) ^ start_clock(), chunks: HashMap::new() }
    }

    /// The game time of the world's tick 0 (`Clock::set_game_base`), from the host's game time and the ECS clock.
    /// A step that runs the schedule advances the clock first and vanilla's entities see the new game time (`ServerLevel.tick`:
    /// `tickTime` before the entities), so the clock of that tick is one more. Redone at every step, so a pause (clock still, game
    /// time moving) is absorbed at the next one; a lent call between steps keeps the base, bar the first.
    fn note_game_time(&mut self, mw: &MobWorld<'_>, step: Option<bool>) {
        let mut clock = self.world.resource_mut::<Clock>();
        if step.is_none() && clock.has_game_base() {
            return;
        }
        let tick = clock.0 as i64 + i64::from(step == Some(true));
        clock.set_game_base(mw.game_time - tick);
    }

    /// Runs `f` over the world with the level lent for the call, and takes the level out again (panics included).
    pub fn lent<R>(&mut self, mw: &MobWorld<'_>, f: impl FnOnce(&mut World) -> R) -> R {
        let level = Level { mw: Mw(lend(mw)), blocks: Blocks(std::ptr::null_mut()), occ: self.occ, half: 0 };
        self.world.insert_resource(level);
        self.note_game_time(mw, None);
        let ran = catch_unwind(AssertUnwindSafe(|| f(&mut self.world)));
        self.world.remove_resource::<Level>();
        match ran {
            Ok(r) => r,
            Err(p) => resume_unwind(p),
        }
    }

    /// Runs the schedule when a player is online or a ticket keeps a chunk ticking (none: nothing is awake), and returns the host's outputs since the last call, in order.
    pub fn step(&mut self, mw: &MobWorld<'_>) -> Vec<HostEvent> {
        self.scan_fresh_names(mw);
        self.world.resource_mut::<view::Entered>().rotate();
        // Entities tick where a player is, and in the chunks a ticket other than a player's keeps entity-ticking (a forced chunk):
        // vanilla ticks those with no player online.
        let ticking = self.world.resource::<Online>().players().next().is_some()
            || self.world.resource::<crate::cow::Index>().zone.has_pins();
        self.note_game_time(mw, Some(ticking));
        if ticking {
            let level = Level { mw: Mw(lend(mw)), blocks: Blocks(std::ptr::null_mut()), occ: self.occ, half: 0 };
            self.world.insert_resource(level);
            let ran = catch_unwind(AssertUnwindSafe(|| self.schedule.run(&mut self.world)));
            self.world.remove_resource::<Level>();
            if let Err(p) = ran {
                resume_unwind(p);
            }
            crate::dragon::fight_tick::drain_out(&mut self.world);
        }
        self.world.clear_trackers();
        let mut out = Vec::new();
        drain_typed(&mut self.world.resource_mut::<Events>().host, &mut out);
        super::channel::drain_lots(&mut self.world, &mut out);
        out
    }

    /// `step` with the natural pass's spawn hooks answered by `spawn` (None: no mod listens).
    pub fn step_with(&mut self, mw: &MobWorld<'_>, spawn: Option<&dyn crate::spawn_mod_host::SpawnModHost>) -> Vec<HostEvent> {
        use crate::spawn_mod_host::{LentSpawnHost, SpawnModCalls};
        self.world.insert_resource(SpawnModCalls(spawn.map(LentSpawnHost::lend)));
        let ran = catch_unwind(AssertUnwindSafe(|| self.step(mw)));
        self.world.insert_resource(SpawnModCalls(None));
        self.world.insert_resource(crate::targeting::LivingVisibility(None));
        match ran {
            Ok(out) => out,
            Err(p) => resume_unwind(p),
        }
    }

    pub fn world(&self) -> &World {
        &self.world
    }

    pub fn world_mut(&mut self) -> &mut World {
        &mut self.world
    }

    /// The game's `living_visibility` answers for the next `step_with` (`None`: no mod listens, nothing is asked); emptied after it.
    pub fn lend_living_visibility(&mut self, host: Option<&dyn crate::targeting::LivingVisibilityHost>) {
        self.world.insert_resource(crate::targeting::LivingVisibility(host.map(crate::targeting::LentVisibility::lend)));
    }

    /// The host's table of an item's prototype attack range, set once at the level's creation.
    pub fn set_prototype_range_table(&mut self, f: crate::rules::melee_attack::PrototypeRangeFn) {
        self.world.insert_resource(crate::rules::melee_attack::PrototypeRange(f));
    }

    /// `SpecialDates.isHalloween` for the host's day (read by the carved pumpkin of `spawn_gear`); the host sets it when the date changes.
    pub fn set_halloween(&mut self, on: bool) {
        self.world.insert_resource(crate::spawn_gear::Halloween(on));
    }

    /// A creature of species `kind` at `at`, armed and indexed; `nbt` (if any) is loaded onto it. None when the species is unknown.
    pub fn spawn(&mut self, mw: &MobWorld<'_>, kind: &str, at: [f64; 3], nbt: Option<&Compound>) -> Option<Entity> {
        self.spawn_for(mw, kind, at, nbt, crate::spawn_reason::SpawnReason::Command)
    }

    /// `spawn` for the reason the host names (`EntitySpawnReason`): kept on the entity (`Spawn`) and told in `Spawned`.
    pub fn spawn_for(&mut self, mw: &MobWorld<'_>, kind: &str, at: [f64; 3], nbt: Option<&Compound>, reason: crate::spawn_reason::SpawnReason) -> Option<Entity> {
        self.spawn_as(mw, kind, at, nbt, reason, None, None, true)
    }

    /// The entity of `sheet` made for `kind` at `at`, its edits applied, neither finalized nor announced: `finalize_prepared` finishes it,
    /// then `announce_prepared` tells it. The variant draw reads the place of `finalize_at` (`finalizeSpawn` runs before `snapTo`, so a
    /// cat reads the origin); None: the place of `at`.
    pub fn spawn_prepared(
        &mut self,
        mw: &MobWorld<'_>,
        kind: &str,
        at: [f64; 3],
        finalize_at: Option<[f64; 3]>,
        reason: crate::spawn_reason::SpawnReason,
        sheet: &crate::spawn_sheet::SpawnSheet,
    ) -> Option<Entity> {
        let shared = match sheet.group {
            Some(crate::spawn_finalize::Group::Ageable { variant, .. } | crate::spawn_finalize::Group::Shaped { variant, .. }) => variant,
            _ => None,
        };
        let e = self.spawn_as(mw, kind, at, None, reason, shared, finalize_at, false)?;
        self.apply_sheet(e, sheet);
        Some(e)
    }

    /// `Spawned` for the entity `e` of `spawn_prepared`, once `finalize_prepared` has run (`addFreshEntity` comes after `finalizeSpawn`).
    pub fn announce_prepared(&mut self, mw: &MobWorld<'_>, e: Entity, reason: crate::spawn_reason::SpawnReason) {
        let natural = reason == crate::spawn_reason::SpawnReason::Natural;
        self.lent(mw, |w| {
            crate::bridge::host_events_more::said_spawned(w, e, natural, reason);
        });
    }

    /// The edits of `sheet` on the new entity `e`, in the order they were made; the persistence last.
    pub(crate) fn apply_sheet(&mut self, e: Entity, sheet: &crate::spawn_sheet::SpawnSheet) {
        use crate::spawn_sheet::AttrEdit;
        if !sheet.passenger_attachments.is_empty() {
            let adult = sheet.passenger_attachments.iter().map(|a| a.map(f64::from)).collect::<Vec<[f64; 3]>>();
            self.world_mut().entity_mut(e).insert(crate::mount::PassengerSeats { adult: std::borrow::Cow::Owned(adult), baby: None });
        }
        for (slot, stack) in &sheet.equipment {
            crate::equip_stack::set(self.world_mut(), e, *slot, stack.clone());
        }
        for (name, edit) in &sheet.attributes {
            match edit {
                AttrEdit::Base(base) => self.set_attribute_base(e, name, *base),
                AttrEdit::ResetBase => self.reset_attribute_base(e, name),
                AttrEdit::Add(m) => self.add_attribute_modifier(e, name, m.clone()),
                AttrEdit::Remove(id) => self.remove_attribute_modifier(e, name, id),
            };
        }
        for tag in &sheet.tags {
            crate::nbt_tag::add(self.world_mut(), e, tag);
        }
        if let Some(name) = &sheet.name {
            self.set_custom_name(e, Some(name.clone()));
        }
        if sheet.persistent {
            crate::despawn::keep(self.world_mut(), e);
        }
    }

    /// `PatrolSpawner.spawnPatrolMember`: a patrol raider is created (`spawn_for`, `Patrol`), a leader is named and takes its patrol target
    /// from the world's origin (`findPatrolTarget` runs before the mob is placed), then it is finalized for the patrol (the leader wears the banner).
    pub fn spawn_patrol_member(&mut self, mw: &MobWorld<'_>, kind: &str, at: [f64; 3], leader: bool, reason: crate::spawn_reason::SpawnReason, special: f32) -> Option<Entity> {
        let e = self.spawn_for(mw, kind, at, None, reason)?;
        if leader {
            crate::raids::set_patrol_leader(&mut self.world, e, true);
            if reason == crate::spawn_reason::SpawnReason::Patrol {
                crate::behaviours::long_distance_patrol::find_target(&mut self.world, e, [0, 0, 0]);
            }
        }
        self.finalize_prepared(mw, e, reason, special, None);
        if leader && reason == crate::spawn_reason::SpawnReason::Patrol {
            crate::behaviours::long_distance_patrol::wake(&mut self.world, e);
        }
        Some(e)
    }

    /// `finalizeSpawn` of the entity `e` made by `spawn_prepared` (or `spawn_for`) for `reason`, with the pack's `group` (the sheet's):
    /// the species' draws, the class's and the gear, the kept draws (follow bonus, left hand, door flag). `special` is the difficulty's
    /// special multiplier at its place. The outcome holds the group for the next member of the pack.
    pub fn finalize_prepared(
        &mut self,
        mw: &MobWorld<'_>,
        e: Entity,
        reason: crate::spawn_reason::SpawnReason,
        special: f32,
        group: Option<crate::spawn_finalize::Group>,
    ) -> crate::spawn_finalize::Outcome {
        self.spawned += 1;
        let mut random = horizon_core::random::LegacyRandom::new(self.spawned as i64);
        crate::spawn_natural::finalize_in(&mut self.world, mw, e, reason, special, group, &mut random)
    }

    /// `group`: the variant the pack's group gives (`Group::Ageable::variant`), taken instead of a draw by a species that shares it.
    /// `announce`: whether `Spawned` is told here (false for a prepared spawn, told by `announce_prepared`).
    fn spawn_as(&mut self, mw: &MobWorld<'_>, kind: &str, at: [f64; 3], nbt: Option<&Compound>, reason: crate::spawn_reason::SpawnReason, group: Option<u8>, draw_at: Option<[f64; 3]>, announce: bool) -> Option<Entity> {
        let natural = reason == crate::spawn_reason::SpawnReason::Natural;
        let table = species_table();
        // A sort with a constructor of its own (the falling block, the TNT...) has no row among the species.
        let sp = table.0.iter().find(|s| s.name == kind);
        if sp.is_none() && !nbt::declared_with(&self.world, kind) {
            return None;
        }
        // `Mob.checkDespawn`: a creature of a `notInPeaceful` type is discarded at once in peaceful, so it is not made.
        if sp.is_some_and(|s| s.not_in_peaceful) && crate::effects::difficulty(&self.world) == 0 {
            return None;
        }
        self.spawned += 1;
        // The counter is mixed (splitmix64): `spawn_species` sets the low bit of the seed, so two consecutive counts would give one random,
        // and a creature summoned twice at the same place would repeat the walk of the first (vanilla draws a `RandomSource` per entity).
        let seed = {
            let mut z = self.spawned.wrapping_add(0x9E37_79B9_7F4A_7C15);
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        };
        self.lent(mw, |w| {
            // A sort with a constructor of its own (an item, an orb, a falling block) is made by it, with its keys.
            if let Some(made) = nbt::spawn_built(w, kind, Vec3::new(at[0], at[1], at[2]), nbt) {
                let e = made?;
                // The keys the constructor did not read (Tags, Motion, NoGravity...) are loaded as for any other kind.
                if let Some(c) = nbt {
                    nbt::load(w, e, c);
                }
                zone::admit(w, &[e]);
                view::mark(w, e);
                if announce {
                    crate::bridge::host_events_more::said_spawned(w, e, natural, reason);
                }
                return Some(e);
            }
            // A cart is built by its own constructor (`Cart`, `Kind`, `Track`), not as a species: its keys (`Motion`...) are loaded on it.
            if let Some((_, genre)) = crate::species::minecart::GENRES.iter().find(|(d, _)| d.name == kind) {
                let e = crate::cart_state::spawn(w, *genre, at, 0.0);
                if let Some(c) = nbt {
                    nbt::load(w, e, c);
                    crate::cart_variants::start_cart(w, e);
                }
                zone::admit(w, &[e]);
                view::mark(w, e);
                if announce {
                    crate::bridge::host_events_more::said_spawned(w, e, natural, reason);
                }
                return Some(e);
            }
            let sp = sp?;
            let e = spawn_armed(w, sp, at[0], at[2], seed);
            // The place asked for, box and movement included: a creature summoned in the air falls (`cow::teleport` raises `Moving`).
            w.get::<Pos>(e)?;
            crate::cow::teleport(w, e, horizon_core::geometry::Vec3::new(at[0], at[1], at[2]));
            crate::spawning::reseat_anchor(w, e, at);
            // `Mob.checkDespawn` covers every mob, whoever made it: the creature gets its rule of going away, as a wild birth does. A
            // `PersistenceRequired` creature far away is kept, not measured first: the marker goes on before the rule is given.
            if nbt.is_some_and(|c| matches!(c.get(&"PersistenceRequired".into()), Some(horizon_core::nbt::Tag::Byte(v)) if *v != 0)) {
                w.entity_mut(e).insert(crate::despawn::Persistent);
            }
            if let Some(def) = crate::spawning::spawn_def(sp) {
                let near = w.resource::<Online>().players().map(|p| (p.x - at[0]).powi(2) + (p.y - at[1]).powi(2) + (p.z - at[2]).powi(2)).fold(f64::INFINITY, f64::min);
                crate::despawn::admit(w, e, def.despawn, near);
            }
            match nbt {
                Some(c) => {
                    crate::cow::forget_birth_draws(w, e, sp);
                    nbt::load(w, e, c);
                    if let Some(horizon_core::nbt::Tag::IntArray(v)) = c.get(&"UUID".into())
                        && let Ok(words) = <[i32; 4]>::try_from(v.as_slice())
                    {
                        crate::bridge::uuids::set_uuid(w, e, words);
                    }
                }
                // `finalizeSpawn`: the variant of a species that draws one, by the biome of the place (one biome here, as `Spawner` has).
                None => {
                    // `Entity.<init>`: the creature is born with a uuid (`Mth.createInsecureUUID`), here from a stream of its own.
                    let mut u = horizon_core::random::LegacyRandom::new((seed as i64) ^ 0x5DEE_CE66_D1CE_4E5B);
                    let (hi, lo) = { use horizon_core::random::RandomSource; (u.next_long(), u.next_long()) };
                    let hi = (hi & !0xF000) | 0x4000;
                    let lo = (lo & 0x3FFF_FFFF_FFFF_FFFF) | i64::MIN;
                    crate::bridge::uuids::set_uuid(w, e, crate::bridge::uuids::to_words(((hi as u64 as u128) << 64) | lo as u64 as u128));
                    let mut r = horizon_core::random::LegacyRandom::new(seed as i64);
                    crate::behaviours::variant::with_place(w, mw, draw_at.unwrap_or(at), |w, place| {
                        crate::behaviours::variant::draw_at_spawn(w, e, group, place, &mut r);
                        crate::behaviours::variant::draw_sound(w, e, &mut r);
                    });
                }
            }
            zone::admit(w, &[e]);
            view::mark(w, e);
            if announce {
                crate::bridge::host_events_more::said_spawned(w, e, natural, reason);
            }
            Some(e)
        })
    }

    /// The entity never despawns on its own (`setPersistenceRequired`), and the spawn caps no longer count it.
    pub fn set_persistent(&mut self, e: Entity) {
        crate::despawn::keep(&mut self.world, e);
    }

    /// `LivingEntity.setItemSlot`: puts `item` (0: empty) in `slot` of `e` (the saddle saddles the mount, armor is refreshed); the entity gets an `Equipment` when it had none.
    pub fn equip(&mut self, e: Entity, slot: Slot, item: u32) {
        crate::equipment::set_slot(&mut self.world, e, slot, item);
    }

    /// `rider` takes a seat of `vehicle` (the first seat steers); false when the seats are taken.
    pub fn mount(&mut self, mw: &MobWorld<'_>, rider: Entity, vehicle: Entity) -> bool {
        self.lent(mw, |w| mount::ride(w, vehicle, Who::Entity(rider)))
    }

    /// The pieces `e` keeps (preserved ones, `death_drops` without a player's kill) leave their slots and are returned with their
    /// damage, for the host to drop them; `seed` rolls the chances.
    pub fn drop_equipment(&mut self, mw: &MobWorld<'_>, e: Entity, seed: i64) -> Vec<(Slot, u32, i32)> {
        self.lent(mw, |w| {
            let killed = crate::credit::killed_by_player(w, e);
            death_drops(w, e, true, killed, |c: f32| c, |_: u32| false, &mut LegacyRandom::new(seed))
        })
    }

    pub fn despawn(&mut self, e: Entity) {
        mount::leave(&mut self.world, e);
        self.world.despawn(e);
    }

    /// The entity that holds the network id `id`.
    pub fn entity_of(&self, id: u32) -> Option<Entity> {
        self.world.get_resource::<NetIds>().and_then(|n| n.entity(id))
    }

    /// The network id of `e`, bound at the first call from `alloc` (a fresh id each time `alloc` gives a free one).
    pub fn id_for(&mut self, e: Entity, alloc: &mut dyn FnMut() -> u32) -> u32 {
        if let Some(id) = ids::id_of(&self.world, e) {
            return id;
        }
        // An event may name an entity that left in the same tick, before it was ever bound: it gets an id nobody holds.
        if self.world.get_entity(e).is_err() {
            return alloc();
        }
        loop {
            let id = alloc();
            if ids::bind(&mut self.world, e, id) {
                return id;
            }
        }
    }

    pub fn drain_modified(&mut self) -> Vec<(Entity, bool)> {
        view::drain(&mut self.world)
    }

    pub fn set_player(&mut self, p: Player, state: PlayerFact) {
        players::set_player(&mut self.world, p, state);
    }

    pub fn remove_player(&mut self, id: u32) {
        players::remove_player(&mut self.world, id);
    }

    /// Queues a hit on `e`; the next step applies it.
    pub fn hurt(&mut self, mw: &MobWorld<'_>, e: Entity, amount: f32, source: HurtSource) {
        let damage = source.damage_type();
        let mut from = None;
        let (cause, by, direct) = match source {
            HurtSource::Generic => (Cause::Magic, None, false),
            HurtSource::Typed { by, from: at, .. } => {
                from = at;
                (if by.is_some() { Cause::Entity } else { cause_of_damage_type(damage) }, by, by.is_some())
            }
            HurtSource::Player(id) => (Cause::Entity, Some(Who::Player(id)), true),
            HurtSource::Entity(id) => (Cause::Entity, self.entity_of(id).map(Who::Entity), true),
            HurtSource::Fall => (Cause::Fall, None, false),
            HurtSource::Fire => (Cause::InFire, None, false),
            HurtSource::Explosion => (Cause::Blast, None, false),
            HurtSource::IndirectMagic { owner } => (Cause::Magic, owner, false),
            HurtSource::Void => (Cause::OutOfWorld, None, false),
        };
        self.lent(mw, |w| w.resource_mut::<DamageQueue>().0.push(Hit { target: e, amount, cause, damage, from, by, direct, power: None, extra: None, projectile: None }));
    }

    /// The player uses its item (`hand` 0: main, else off hand) on `e`; true when a goal answered.
    pub fn interact(&mut self, mw: &MobWorld<'_>, e: Entity, player: u32, hand: u8) -> bool {
        let Some(p) = self.world.resource::<Online>().get(player) else { return false };
        let item = if hand == 0 { p.main } else { p.off };
        self.lent(mw, |w| goals::interact(w, e, &Interaction { player, item }))
    }

    /// `interact`, the rules being given the whole stack used (`used_stack::UsedStack`) for the length of the call.
    pub fn interact_with(&mut self, mw: &MobWorld<'_>, e: Entity, player: u32, hand: u8, stack: myriade_engine_lib::items::ItemStack, creative: bool) -> bool {
        self.world.insert_resource(crate::used_stack::UsedStack { stack, creative });
        let used = self.interact(mw, e, player, hand);
        self.world.remove_resource::<crate::used_stack::UsedStack>();
        used
    }

    /// Moves `e` to `at`, and its box with it: its index cell is read again.
    pub fn teleport(&mut self, mw: &MobWorld<'_>, e: Entity, at: [f64; 3]) {
        let Some(old) = self.world.get::<Pos>(e).map(|p| p.0) else { return };
        let to = Vec3::new(at[0], at[1], at[2]);
        let d = [at[0] - old.x, at[1] - old.y, at[2] - old.z];
        if let Some(b) = self.world.get::<crate::cow::Bounds>(e).copied() {
            let (mn, mx) = (b.0.min_position(), b.0.max_position());
            let moved = Aabb::new(Vec3::new(mn.x + d[0], mn.y + d[1], mn.z + d[2]), Vec3::new(mx.x + d[0], mx.y + d[1], mx.z + d[2]));
            self.world.entity_mut(e).insert(crate::cow::Bounds(moved));
        }
        self.world.entity_mut(e).insert(Pos(to));
        self.lent(mw, |w| {
            crate::cow::enter_index(w, e);
            view::mark(w, e);
        });
    }

    /// A block changed on the host: the cows near it look again at it at the next step.
    pub fn set_block_event(&mut self, pos: [i32; 3]) {
        // `block_changes` reads it again at the next step: masks, hazards, `ChangedBlocks`, the creatures and the paths beside it.
        self.world.resource_mut::<crate::cow::HostWrites>().0.push((pos[0], pos[1], pos[2]));
        // The creakings linked to a heart at `pos` ask it again (`host_creaking::ask_heart`); a lookup, nothing scanned.
        let linked = self.world.get_resource::<super::host_creaking::HeartWatch>().and_then(|m| m.0.get(&pos)).cloned();
        for e in linked.into_iter().flatten() {
            super::host_creaking::ask_heart(&mut self.world, e);
        }
    }

    /// The blows the creatures dealt to players since the last call (`PlayerState.hits`): the player, the amount, where it came from
    /// and the creature that dealt it. The host applies them (armour, effects, knock-back, death).
    pub fn take_player_hits(&mut self) -> Vec<(u32, f32, [f64; 3], Option<Entity>, Option<&'static str>)> {
        let hits = std::mem::take(&mut self.world.resource_mut::<crate::hostile::PlayerState>().hits);
        // The host owns the health: the copy counted down by these blows goes back to what the host last said (`PlayerFact.alive`).
        for h in &hits {
            let alive = self.world.get_resource::<super::players::PlayerFacts>().and_then(|f| f.0.get(&h.player)).is_none_or(|f| f.alive);
            let mut ps = self.world.resource_mut::<crate::hostile::PlayerState>();
            if let Some(v) = ps.health.get_mut(h.player as usize) {
                *v = if alive { 20.0 } else { 0.0 };
            }
        }
        hits.into_iter()
            .map(|h| {
                let by = match h.by {
                    Some(Who::Entity(e)) => Some(e),
                    _ => None,
                };
                let y = by.and_then(|e| self.world.get::<Pos>(e)).map_or(0.0, |p| p.0.y);
                (h.player, h.amount, [h.from.0, y, h.from.1], by, h.kind)
            })
            .collect()
    }

    /// The second knockbacks of the blows (`causeExtraKnockback`) since the last call: the player, the position of the creature that
    /// dealt it, (x, z, power) and that creature. The host applies them after the blow (`take_player_hits`).
    pub fn take_player_pushes(&mut self) -> Vec<(u32, [f64; 3], (f64, f64, f64), Entity)> {
        let pushes = std::mem::take(&mut self.world.resource_mut::<crate::hostile::PlayerState>().pushes);
        pushes
            .into_iter()
            .map(|(player, extra, by)| {
                let at = self.world.get::<Pos>(by).map_or([0.0; 3], |p| [p.0.x, p.0.y, p.0.z]);
                (player, at, extra, by)
            })
            .collect()
    }

    /// The host rewrote the blocks of a chunk in bulk (`World::edit_runs`: a `/fill`, a mod's edit): the masks are dropped
    /// (`Occupancy::clear`), the paths are stale, and the creatures of the chunk look at their ground again.
    pub fn chunk_rewritten(&mut self, cx: i32, cz: i32) {
        self.occ.clear();
        crate::pathsearch::layer_bump();
        for e in self.chunk_entities(cx, cz) {
            if self.world.get_entity(e).is_ok_and(|r| r.contains::<Disabled>()) {
                continue;
            }
            if let Some(mut c) = self.world.get_mut::<crate::cow::Contact>(e) {
                c.on_ground = false;
            }
            if let Some(mut b) = self.world.get_mut::<crate::cow::Bounds>(e) {
                b.set_changed();
            }
            self.world.entity_mut(e).insert((crate::cow::Moving, crate::cow::Recheck));
        }
    }

    /// A chunk just loaded (with or without entities): its record is made in the index (before any of its entities comes), its water, lava,
    /// fire, cactus, powder snow and turtle eggs join its hazard columns, and its points and watched blocks are read.
    pub fn chunk_hazards(&mut self, mw: &MobWorld<'_>, cx: i32, cz: i32) {
        self.world.resource_scope(|w, mut idx: Mut<crate::cow::Index>| {
            idx.chunk_created(cx, cz);
            w.resource_mut::<crate::cow::Hazards>().scan_chunk(&mut idx.chunks, mw, cx, cz);
        });
        self.world.resource_mut::<crate::poi::PoiRegistry>().scan_chunk(mw, cx, cz);
        self.world.resource_mut::<crate::block_watch::BlockWatch>().scan_chunk(mw, cx, cz);
    }

    /// The blocks named since the last call (`BlockWatch::watch`) are read from every loaded chunk (a rare event: nothing when none is new).
    fn scan_fresh_names(&mut self, mw: &MobWorld<'_>) {
        let fresh = self.world.resource_mut::<crate::block_watch::BlockWatch>().bypass_change_detection().take_fresh();
        if fresh.is_empty() {
            return;
        }
        let idx = self.world.resource::<crate::cow::Index>();
        let loaded: Vec<(i32, i32)> = idx.chunks.live().map(|s| (idx.chunks.recs[s as usize].cx, idx.chunks.recs[s as usize].cz)).collect();
        let mut watch = self.world.resource_mut::<crate::block_watch::BlockWatch>();
        for (cx, cz) in loaded {
            watch.scan_chunk_names(mw, cx, cz, &fresh);
        }
    }

    /// Restores the NBT the chunk was saved with (its entities come back; none when it was never unloaded).
    pub fn chunk_loaded(&mut self, mw: &MobWorld<'_>, cx: i32, cz: i32) {
        let saved = self.chunks.remove(&(cx, cz)).unwrap_or_default();
        // The record is made by `chunk_hazards` before this call; made here when it was not, so the entities find their chunk.
        self.world.resource_mut::<crate::cow::Index>().chunk_created(cx, cz);
        self.lent(mw, |w| {
            for c in &saved {
                nbt::restore(w, c);
            }
        });
    }

    /// Puts the chunk's creatures to sleep, keeps their NBT (a passenger inside its mount's), and takes them out of the world.
    pub fn chunk_unloaded(&mut self, mw: &MobWorld<'_>, cx: i32, cz: i32) {
        let es = self.chunk_entities(cx, cz);
        let saved: Vec<Compound> = self.lent(mw, |w| {
            let now = w.resource::<Clock>().0;
            for &e in &es {
                if w.get_entity(e).is_ok_and(|r| !r.contains::<Disabled>()) {
                    zone::sleep(w, e, now);
                }
            }
            es.iter().filter(|&&e| w.get::<Riding>(e).is_none() && w.get::<crate::seat::Seated>(e).is_none()).filter_map(|&e| nbt::save(w, e)).collect()
        });
        for e in es {
            self.world.despawn(e);
        }
        self.chunks.insert((cx, cz), saved);
        let left = self.world.resource_scope(|w, mut idx: Mut<crate::cow::Index>| {
            w.resource_mut::<crate::cow::Hazards>().forget_chunk(&idx.chunks, cx, cz);
            idx.chunk_unloaded(cx, cz)
        });
        // What was still indexed in the chunk (an entity that is not saved with it) is no longer indexed: it waits, asleep, out of the zone.
        if !left.is_empty() {
            let now = self.world.resource::<Clock>().0;
            for e in left {
                if let Some(mut c) = self.world.get_mut::<crate::cow::Cell>(e) {
                    c.0 = u32::MAX;
                    if self.world.get_entity(e).is_ok_and(|r| !r.contains::<Disabled>()) {
                        zone::sleep(&mut self.world, e, now);
                    }
                }
            }
        }
        self.world.resource_mut::<crate::poi::PoiRegistry>().forget_chunk(cx, cz);
        self.world.resource_mut::<crate::block_watch::BlockWatch>().forget_chunk(cx, cz);
        nbt::forget_chunk(&mut self.world, cx, cz);
    }

    /// A chunk loaded in the world whose entities are not accessible yet (its ticket level is below `FULL`): their NBT waits here,
    /// saved with the chunk, until `chunk_loaded` makes them.
    pub fn hold_chunk(&mut self, cx: i32, cz: i32, entities: Vec<Compound>) {
        if !entities.is_empty() {
            self.chunks.entry((cx, cz)).or_default().extend(entities);
        }
    }

    /// The chunk stays loaded but its entities leave the level (its ticket level fell below `FULL`): their NBT is kept as
    /// `chunk_unloaded` keeps it, and `chunk_loaded` brings them back.
    pub fn chunk_detach(&mut self, mw: &MobWorld<'_>, cx: i32, cz: i32) {
        let es = self.chunk_entities(cx, cz);
        if es.is_empty() {
            return;
        }
        let saved: Vec<Compound> = self.lent(mw, |w| {
            es.iter().filter(|&&e| w.get::<Riding>(e).is_none() && w.get::<crate::seat::Seated>(e).is_none()).filter_map(|&e| nbt::save(w, e)).collect()
        });
        for e in es {
            self.world.despawn(e);
        }
        self.chunks.entry((cx, cz)).or_default().extend(saved);
    }

    /// The NBT of the saved entities of the chunk, and of those it holds out of the level; a passenger is written inside its mount (`Passengers`), not on its own.
    /// Whether `save_chunk` would give something: a saved entity (not a rider) stands in the chunk, or the chunk holds NBT. Writes and
    /// builds nothing.
    pub fn chunk_holds(&mut self, cx: i32, cz: i32) -> bool {
        if self.chunks.get(&(cx, cz)).is_some_and(|h| !h.is_empty()) {
            return true;
        }
        let mut q = self.world.query_filtered::<(&Pos, Has<Riding>, Has<crate::seat::Seated>), (With<Saved>, Allow<Disabled>)>();
        q.iter(&self.world).any(|(p, riding, seated)| !riding && !seated && chunk_of(p.0) == (cx, cz))
    }
    pub fn save_chunk(&mut self, cx: i32, cz: i32) -> Vec<Compound> {
        let es = self.chunk_entities(cx, cz);
        let mut out: Vec<Compound> = es.into_iter().filter(|&e| self.world.get::<Riding>(e).is_none() && self.world.get::<crate::seat::Seated>(e).is_none()).filter_map(|e| nbt::save(&self.world, e)).collect();
        if let Some(held) = self.chunks.get(&(cx, cz)) {
            out.extend(held.iter().cloned());
        }
        out
    }

    /// The entity made from `c` (its `id` and `Pos`), None when no recipe makes it.
    pub fn load_entity(&mut self, mw: &MobWorld<'_>, c: &Compound) -> Option<Entity> {
        self.lent(mw, |w| nbt::restore(w, c))
    }

    /// `load_entity` for a mount with its riders: the entity of each compound of the NBT in its order (the root, then each rider
    /// depth first), None for one that was not made.
    pub fn load_entity_tree(&mut self, mw: &MobWorld<'_>, c: &Compound) -> Vec<Option<Entity>> {
        self.lent(mw, |w| nbt::restore_tree(w, c))
    }

    /// `root` and its creature riders all the way down, in the order `Passengers` is written (depth first, root first).
    pub fn ride_tree(&self, root: Entity) -> Vec<Entity> {
        let w = self.world();
        let mut tree = Vec::new();
        let mut todo = vec![root];
        while let Some(e) = todo.pop() {
            tree.push(e);
            todo.extend(crate::nbt_fields::rides::passengers_of(w, e).into_iter().rev());
        }
        tree
    }

    pub fn entity_nbt(&self, e: Entity) -> Option<Compound> {
        nbt::save(&self.world, e)
    }

    /// Loads `patch` onto `e`; false when `e` is not saved.
    pub fn merge_nbt(&mut self, e: Entity, patch: &Compound) -> bool {
        nbt::load(&mut self.world, e, patch).is_some()
    }

    pub fn select(&self, q: &Select) -> Vec<Entity> {
        select::select(&self.world, q)
    }

    /// The saved entities whose position is in chunk (`cx`, `cz`), sleeping or not, riders included.
    fn chunk_entities(&mut self, cx: i32, cz: i32) -> Vec<Entity> {
        let mut q = self.world.query_filtered::<(Entity, &Pos), (With<Saved>, Allow<Disabled>)>();
        q.iter(&self.world).filter(|(_, p)| chunk_of(p.0) == (cx, cz)).map(|(e, _)| e).collect()
    }
}
