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
use crate::cow::{Blocks, Cause, Clock, DamageQueue, Hit, Level, Mw, Pos, host_level, spawn_armed, species_table};
use crate::equipment_drop::death_drops;
pub use super::host_read_living::Flag;
use crate::goals::{self, Equipment, Events, Online, Player, Slot};
use crate::host_out::HostEvent;
use crate::mount::{self, Riding};
use crate::nbt::{self, Saved};
use crate::targeting::Who;
use crate::zone;
use bevy_ecs::{entity::Entity, entity_disabling::Disabled, prelude::*, query::Allow};
use horizon_client_lib::mobs::{MobTypes, MobWorld};
use horizon_core::{geometry::{Aabb, Vec3}, nbt::Compound, random::LegacyRandom};
use std::{
    collections::HashMap,
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
};

/// The half-width of the spatial index, in blocks: entities beyond it are read as on its border.
const HALF: i32 = 1024;

/// What a host world starts with: the seed of its spawns, the dimension's name, and the simulation distance in chunks (0: none).
#[derive(Clone, Debug)]
pub struct HostSetup {
    pub seed: i64,
    pub dimension: &'static str,
    pub simulation_radius: i32,
}

/// Who or what hurt an entity, as the host names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HurtSource {
    Generic,
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
    half: i32,
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

impl EcsWorld {
    /// `tags` is the resources' folder: the entity type tags are read from it, against the species table.
    pub fn new(setup: &HostSetup, mw: &MobWorld<'_>, types: &'static MobTypes, tags: &std::path::Path) -> EcsWorld {
        let level = host_level(lend(mw), types, HALF);
        let occ = level.occ;
        crate::type_tags::install(tags, &species_table().0);
        let mut world = assemble::assemble(level);
        world.remove_resource::<Level>();
        zone::enable(&mut world, setup.simulation_radius);
        world.insert_resource(crate::frame_item::DropRng(crate::flat::Rng(setup.seed as u64 ^ 0x9E37_79B9_7F4A_7C15)));
        EcsWorld { world, schedule: crate::cow::schedule(), occ, half: HALF, spawned: setup.seed as u64, chunks: HashMap::new() }
    }

    /// Runs `f` over the world with the level lent for the call, and takes the level out again (panics included).
    pub(super) fn lent<R>(&mut self, mw: &MobWorld<'_>, f: impl FnOnce(&mut World) -> R) -> R {
        let level = Level { mw: Mw(lend(mw)), blocks: Blocks(std::ptr::null_mut()), occ: self.occ, half: self.half };
        self.world.insert_resource(level);
        let ran = catch_unwind(AssertUnwindSafe(|| f(&mut self.world)));
        self.world.remove_resource::<Level>();
        match ran {
            Ok(r) => r,
            Err(p) => resume_unwind(p),
        }
    }

    /// Runs the schedule when a player is online (none: nothing is awake), and returns the host's outputs since the last call, in order.
    pub fn step(&mut self, mw: &MobWorld<'_>) -> Vec<HostEvent> {
        if self.world.resource::<Online>().players().next().is_some() {
            let level = Level { mw: Mw(lend(mw)), blocks: Blocks(std::ptr::null_mut()), occ: self.occ, half: self.half };
            self.world.insert_resource(level);
            let ran = catch_unwind(AssertUnwindSafe(|| self.schedule.run(&mut self.world)));
            self.world.remove_resource::<Level>();
            if let Err(p) = ran {
                resume_unwind(p);
            }
        }
        self.world.clear_trackers();
        let mut out = Vec::new();
        drain_typed(&mut self.world.resource_mut::<Events>().host, &mut out);
        super::channel::drain_lots(&mut self.world, &mut out);
        out
    }

    pub fn world(&self) -> &World {
        &self.world
    }

    pub fn world_mut(&mut self) -> &mut World {
        &mut self.world
    }

    /// A creature of species `kind` at `at`, armed and indexed; `nbt` (if any) is loaded onto it. None when the species is unknown.
    pub fn spawn(&mut self, mw: &MobWorld<'_>, kind: &str, at: [f64; 3], nbt: Option<&Compound>) -> Option<Entity> {
        self.spawn_as(mw, kind, at, nbt, false)
    }

    /// `spawn` for a creature of the host's natural spawning pass: its `Spawned` event says so (`natural`).
    pub fn spawn_natural(&mut self, mw: &MobWorld<'_>, kind: &str, at: [f64; 3]) -> Option<Entity> {
        self.spawn_as(mw, kind, at, None, true)
    }

    fn spawn_as(&mut self, mw: &MobWorld<'_>, kind: &str, at: [f64; 3], nbt: Option<&Compound>, natural: bool) -> Option<Entity> {
        let table = species_table();
        let sp = table.0.iter().find(|s| s.name == kind)?;
        self.spawned += 1;
        let seed = self.spawned;
        self.lent(mw, |w| {
            let e = spawn_armed(w, sp, at[0], at[2], seed);
            w.get_mut::<Pos>(e)?.0.y = at[1];
            if let Some(c) = nbt {
                nbt::load(w, e, c);
            }
            zone::admit(w, &[e]);
            view::mark(w, e);
            crate::bridge::host_events_more::said_spawned(w, e, natural);
            Some(e)
        })
    }

    /// The entity never despawns on its own (`setPersistenceRequired`), and the spawn caps no longer count it.
    pub fn set_persistent(&mut self, e: Entity) {
        crate::despawn::keep(&mut self.world, e);
    }

    /// Puts `item` (0: empty) in `slot` of `e`; the entity gets an `Equipment` when it had none. A slot the entity lacks is ignored.
    pub fn equip(&mut self, e: Entity, slot: Slot, item: u32) {
        if self.world.get::<Equipment>(e).is_none() {
            self.world.entity_mut(e).insert(Equipment::default());
        }
        let Some(mut q) = self.world.get_mut::<Equipment>(e) else { return };
        let f = match slot {
            Slot::Head => &mut q.head,
            Slot::Chest => &mut q.chest,
            Slot::Legs => &mut q.legs,
            Slot::Feet => &mut q.feet,
            Slot::Main => &mut q.main,
            Slot::Off => &mut q.off,
            Slot::Body => &mut q.body,
            Slot::Saddle => &mut q.saddle,
        };
        *f = item;
    }

    /// `rider` takes a seat of `vehicle` (the first seat steers); false when the seats are taken.
    pub fn mount(&mut self, mw: &MobWorld<'_>, rider: Entity, vehicle: Entity) -> bool {
        self.lent(mw, |w| mount::ride(w, vehicle, Who::Entity(rider)))
    }

    /// The pieces `e` keeps (preserved ones, `death_drops` without a player's kill) leave their slots and are returned with their
    /// damage, for the host to drop them; `seed` rolls the chances.
    pub fn drop_equipment(&mut self, mw: &MobWorld<'_>, e: Entity, seed: i64) -> Vec<(Slot, u32, i32)> {
        self.lent(mw, |w| death_drops(w, e, true, false, |c: f32| c, |_: u32| false, &mut LegacyRandom::new(seed)))
    }

    pub fn despawn(&mut self, e: Entity) {
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
        loop {
            let id = alloc();
            if ids::bind(&mut self.world, e, id) {
                return id;
            }
        }
    }

    pub fn drain_modified(&mut self) -> Vec<Entity> {
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
        let (cause, by, direct) = match source {
            HurtSource::Generic => (Cause::Magic, None, false),
            HurtSource::Player(id) => (Cause::Entity, Some(Who::Player(id)), true),
            HurtSource::Entity(id) => (Cause::Entity, self.entity_of(id).map(Who::Entity), true),
            HurtSource::Fall => (Cause::Fall, None, false),
            HurtSource::Fire => (Cause::InFire, None, false),
            HurtSource::Explosion => (Cause::Blast, None, false),
            HurtSource::IndirectMagic { owner } => (Cause::Magic, owner, false),
            HurtSource::Void => (Cause::OutOfWorld, None, false),
        };
        self.lent(mw, |w| w.resource_mut::<DamageQueue>().0.push(Hit { target: e, amount, cause, damage, from: None, by, direct, power: None }));
    }

    /// The player uses its item (`hand` 0: main, else off hand) on `e`; true when a goal answered.
    pub fn interact(&mut self, mw: &MobWorld<'_>, e: Entity, player: u32, hand: u8) -> bool {
        let Some(p) = self.world.resource::<Online>().get(player) else { return false };
        let item = if hand == 0 { p.main } else { p.off };
        self.lent(mw, |w| goals::interact(w, e, &Interaction { player, item }))
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
        self.world.resource_mut::<crate::hostile::ChangedBlocks>().0.push((pos[0], pos[1], pos[2]));
    }

    /// Restores the NBT the chunk was saved with (its entities come back; none when it was never unloaded).
    pub fn chunk_loaded(&mut self, mw: &MobWorld<'_>, cx: i32, cz: i32) {
        let saved = self.chunks.remove(&(cx, cz)).unwrap_or_default();
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
            es.iter().filter(|&&e| w.get::<Riding>(e).is_none()).filter_map(|&e| nbt::save(w, e)).collect()
        });
        for e in es {
            self.world.despawn(e);
        }
        self.chunks.insert((cx, cz), saved);
        nbt::forget_chunk(&mut self.world, cx, cz);
    }

    /// The NBT of the saved entities of the chunk; a passenger is written inside its mount (`Passengers`), not on its own.
    pub fn save_chunk(&mut self, cx: i32, cz: i32) -> Vec<Compound> {
        let es = self.chunk_entities(cx, cz);
        es.into_iter().filter(|&e| self.world.get::<Riding>(e).is_none()).filter_map(|e| nbt::save(&self.world, e)).collect()
    }

    /// The entity made from `c` (its `id` and `Pos`), None when no recipe makes it.
    pub fn load_entity(&mut self, mw: &MobWorld<'_>, c: &Compound) -> Option<Entity> {
        self.lent(mw, |w| nbt::restore(w, c))
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
