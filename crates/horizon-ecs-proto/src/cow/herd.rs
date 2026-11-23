//! The herd harness: a world with the schedule and the helpers to step it.
//! A cow as data components and batch systems (palier 1): the stroll goal (a geometric wait with the law of
//! `nextInt(60) == 0` each tick, then `LandRandomPos` 10 x 7), the move control, `LivingEntity.travel` on the
//! ground with Horizon's real block collision sweep (`SceneCollisions::resolve_in_border_with`), and sleeping.
//! Species data (size, step, speed, stroll range) are component values, not branches.
#[allow(unused_imports)]
use crate::IFloor as _;
#[allow(unused_imports)]
use bevy_ecs::{
    entity_disabling::Disabled,
    prelude::*,
    query::Allow,
    schedule::{Schedule, ScheduleLabel, SingleThreadedExecutor},
};
#[allow(unused_imports)]
use myriade_engine_lib::mobs::{
    MobWorld,
    nav::Navigation,
    path::{Evaluator, Finder, Path, PathMob, PATH_TYPES},
};
#[allow(unused_imports)]
use myriade_level::{chunk::Chunk, collisions::SceneCollisions};
#[allow(unused_imports)]
use horizon_core::geometry::{Aabb, Vec3, trig};
#[allow(unused_imports)]
use std::sync::Arc;
use super::*;

pub struct Herd {
    pub world: World,
    pub schedule: Schedule,
}

impl Herd {
    // ---- the constructors of a server: the zone of simulation is ON (no player registered: every creature sleeps) --------------
    // The `*_awake` ones below build a herd WITHOUT the zone (every creature ticks as if a player stood within range): a harness
    // that wants it asks for it by name; it is never the state a server reaches by having no player.
    /// The simulation distance (in chunks) of a herd built by the constructors without `_awake` (the game's default).
    pub const SIMULATION_RADIUS: i32 = 10;
    fn zoned(mut self) -> Self {
        self.set_simulation(Self::SIMULATION_RADIUS);
        self
    }
    /// `n` cows spaced `spacing` blocks apart, the zone on.
    pub fn new(n: usize, spacing: f64, sleep: bool, parallel: bool, seed: u64) -> Self {
        Self::new_awake(n, spacing, sleep, parallel, seed).zoned()
    }
    pub fn with_hazards(n: usize, spacing: f64, sleep: bool, parallel: bool, seed: u64, hazards: bool) -> Self {
        Self::with_hazards_awake(n, spacing, sleep, parallel, seed, hazards).zoned()
    }
    pub fn with_mix(n: usize, spacing: f64, sleep: bool, parallel: bool, seed: u64, hazards: bool, other_every: usize) -> Self {
        Self::with_mix_awake(n, spacing, sleep, parallel, seed, hazards, other_every).zoned()
    }
    pub fn with_slab(half: i32, sleep: bool, hazards: bool) -> Self {
        Self::with_slab_awake(half, sleep, hazards).zoned()
    }
    pub fn with_spawn_world_kind(half: i32, sleep: bool, kind: crate::spawning::Kind) -> Self {
        Self::with_spawn_world_kind_awake(half, sleep, kind).zoned()
    }
    pub fn with_spawn_world(half: i32, sleep: bool) -> Self {
        Self::with_spawn_world_awake(half, sleep).zoned()
    }
    /// `n` cows spaced `spacing` blocks apart on a slab just large enough.
    pub fn new_awake(n: usize, spacing: f64, sleep: bool, parallel: bool, seed: u64) -> Self {
        Self::with_hazards_awake(n, spacing, sleep, parallel, seed, false)
    }
    /// With `hazards`, 3 x 3 patches of lava, water, powder snow, fire and cactus every 48 blocks.
    pub fn with_hazards_awake(n: usize, spacing: f64, sleep: bool, parallel: bool, seed: u64, hazards: bool) -> Self {
        Self::with_mix_awake(n, spacing, sleep, parallel, seed, hazards, 0)
    }
    /// A mixed herd: every `other_every`-th animal is of the second species (the pig), the others of the first (the
    /// cow); 0: all of the process's default species.
    pub fn with_mix_awake(n: usize, spacing: f64, sleep: bool, parallel: bool, seed: u64, hazards: bool, other_every: usize) -> Self {
        let mut herd = Self::build(n, spacing, sleep, parallel, seed, hazards, other_every, None, false);
        // The loot tables are read once, now: the first death of a bench would read them in the middle of a measure.
        crate::loot::preload(&mut herd.world);
        herd
    }
    /// An empty walled slab of `half` blocks each way.
    pub fn with_slab_awake(half: i32, sleep: bool, hazards: bool) -> Self {
        Self::build(0, 1.0, sleep, false, 5, hazards, 0, Some(half), false)
    }
    /// Day or night for the spawn rules (`MobWorld.daylight`: the sky darkens by 11 at night); the world is the same one, read again.
    pub fn set_daylight(&mut self, day: bool) {
        let mut level = *self.world.resource::<Level>();
        let w: &'static MobWorld<'static> = Box::leak(Box::new(MobWorld { daylight: day, ..level.mw.0.clone() }));
        level.mw = Mw(w);
        *self.world.resource_mut::<Level>() = level;
    }
    /// Rain or none (`MobWorld.raining`, the weather the fishing hook reads); the world is the same one, read again.
    pub fn set_raining(&mut self, raining: bool) {
        let mut level = *self.world.resource::<Level>();
        let w: &'static MobWorld<'static> = Box::leak(Box::new(MobWorld { raining, ..level.mw.0.clone() }));
        level.mw = Mw(w);
        *self.world.resource_mut::<Level>() = level;
    }
    /// The `maxEntityCramming` game rule (`MobWorld.max_entity_cramming`); the world is the same one, read again.
    pub fn set_max_entity_cramming(&mut self, max: i32) {
        let mut level = *self.world.resource::<Level>();
        let w: &'static MobWorld<'static> = Box::leak(Box::new(MobWorld { max_entity_cramming: max, ..level.mw.0.clone() }));
        level.mw = Mw(w);
        *self.world.resource_mut::<Level>() = level;
    }
    /// The dimension the world is read as (`MobWorld.dimension_type`: its ambient light lifts the brightness, `MobWorld.brightness`); the world is the same one, read again.
    pub fn set_dimension(&mut self, dimension: &'static horizon_world::dimension_type::DimensionFacts) {
        let mut level = *self.world.resource::<Level>();
        let w: &'static MobWorld<'static> = Box::leak(Box::new(MobWorld { dimension_type: dimension, ..level.mw.0.clone() }));
        level.mw = Mw(w);
        *self.world.resource_mut::<Level>() = level;
    }
    /// Turns the natural spawning on, with the spawn lists of every biome of the world (the game's data, by biome index) and the seed of
    /// its random. The biome of a position is read from the world, so `_biome` (the harness's one biome) is not used.
    pub fn enable_spawning(&mut self, _biome: &str, seed: i64) {
        let mw = self.world.resource::<Level>().mw.0;
        // The rules of a harness world: mobs and monsters may spawn (the host writes them when they change).
        install_spawning(&mut self.world, mw, seed, (true, true), None);
    }
    /// An empty slab of `half` blocks each way with a real surface (grass on dirt on stone down to the bottom of the world) for the
    /// natural spawning (`spawning.rs`): the surface is the block y = 0, the creatures stand at y = 1.
    pub fn with_spawn_world_kind_awake(half: i32, sleep: bool, kind: crate::spawning::Kind) -> Self {
        // The kind is read by the builder: no other world is built meanwhile.
        static BUILDING: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = BUILDING.lock().unwrap_or_else(|e| e.into_inner());
        crate::spawning::set_kind(kind);
        let mut h = Self::build(0, 1.0, sleep, false, 5, false, 0, Some(half), true);
        if kind == crate::spawning::Kind::TwoBiomes {
            crate::spawning::split_biomes(&mut h.world);
        }
        crate::loot::preload(&mut h.world);
        h
    }
    pub fn with_spawn_world_awake(half: i32, sleep: bool) -> Self {
        Self::with_spawn_world_kind_awake(half, sleep, crate::spawning::Kind::Plains)
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn build(n: usize, spacing: f64, sleep: bool, parallel: bool, seed: u64, hazards: bool, other_every: usize, slab: Option<i32>, spawn_world: bool) -> Self {
        let side = (n as f64).sqrt().ceil() as i32;
        let half = slab.unwrap_or(((f64::from(side) * spacing / 2.0) as i32 + 6).max(20));
        let mut world = World::new();
        crate::goals::reset_uids();
        let level = if spawn_world { build_level_neutral_with(half, crate::spawning::world_builder(half)) } else { build_level_neutral(half, hazards) };
        assemble_resources(&mut world, level, sleep, parallel);
        let mut rng = Rng(seed | 1);
        for i in 0..n {
            let (cx, cz) = ((i as i32 % side) as f64, (i as i32 / side) as f64);
            let o = -f64::from(side) * spacing / 2.0;
            let s = u64::from(rng.next_u32()) << 32 | u64::from(rng.next_u32());
            let table = species_table();
            let sp = if other_every > 0 { table.0[usize::from(i % other_every == 0)] } else { *species() };
            spawn_species(&mut world, &sp, o + cx * spacing + 0.5, o + cz * spacing + 0.5, s);
        }
        {
            let mut arm = Rng(seed ^ 0x9e37_79b9_7f4a_7c15 | 1);
            let ids: Vec<Entity> = world.query_filtered::<Entity, With<Stroll>>().iter(&world).collect();
            for e in ids {
                let at = 1 + idle_wait(&mut arm, 60).max(0) as u64;
                world.resource_mut::<Wheel>().push(at, e, Due::Stroll);
                world.get_mut::<Stroll>(e).unwrap().scheduled = true;
            }
        }
        crate::goals::arm_all(&mut world);
        // The cows are listed in the index from the start (a player event is read at the first tick).
        world.run_system_cached(reindex).unwrap();
        Self { world, schedule: schedule() }
    }
    /// Adds animals of the process's species at `at` (x, z), their timers armed as `build` arms them (call it once, on a herd
    /// that has none: `arm_all` arms every animal).
    pub fn populate(&mut self, at: &[(f64, f64)], seed: u64) -> Vec<Entity> {
        let sp = *species();
        let mut rng = Rng(seed | 1);
        let mut out = Vec::with_capacity(at.len());
        for &(x, z) in at {
            let s = u64::from(rng.next_u32()) << 32 | u64::from(rng.next_u32());
            out.push(spawn_species(&mut self.world, &sp, x, z, s));
        }
        let mut arm = Rng(seed ^ 0x9e37_79b9_7f4a_7c15 | 1);
        for &e in &out {
            let now = now_of(&self.world, e);
            let at = now + 1 + idle_wait(&mut arm, 60).max(0) as u64;
            arm_at(&mut self.world, e, at, Due::Stroll);
            if let Some(mut st) = self.world.get_mut::<Stroll>(e) {
                st.scheduled = true;
            }
        }
        crate::goals::arm_all(&mut self.world);
        self.world.run_system_cached(reindex).unwrap();
        out
    }
    /// Lists in the index and arms creatures spawned by hand (`spawn_species`), then lets the zone put to sleep those whose chunk does not tick.
    pub fn admit_spawned(&mut self, es: &[Entity]) {
        crate::goals::arm_all(&mut self.world);
        self.world.run_system_cached(reindex).unwrap();
        crate::zone::admit(&mut self.world, es);
    }
    /// Micro-bench of the path search, 1: every animal gets a target 1.5 blocks away and `NeedsPath`.
    pub fn bench_pathfind_prepare(&mut self) {
        let ids: Vec<Entity> = self.world.query_filtered::<Entity, With<Stroll>>().iter(&self.world).collect();
        for &e in &ids {
            let p = self.world.get::<Pos>(e).unwrap().0;
            let mut s = self.world.get_mut::<Stroll>(e).unwrap();
            (s.tx, s.tz, s.ty) = (p.x + 1.5, p.z, p.y.ifloor());
            self.world.get_mut::<Nav>(e).unwrap().speed = 1.0;
            self.world.entity_mut(e).insert(NeedsPath);
        }
    }
    /// 2: `pathfind` alone, once; the searches it ran.
    pub fn bench_pathfind_run(&mut self) -> usize {
        let before: u32 = self.world.query::<&Nav>().iter(&self.world).map(|n| n.searches).sum();
        self.world.run_system_cached(pathfind).unwrap();
        let after: u32 = self.world.query::<&Nav>().iter(&self.world).map(|n| n.searches).sum();
        (after - before) as usize
    }
    /// 2b: the same searches, the `Finder` alone from the same data (no system around it); the searches run.
    pub fn bench_finder_only(&mut self) -> usize {
        let level = *self.world.resource::<Level>();
        let mw = level.mw.0;
        let ids: Vec<Entity> = self.world.query_filtered::<Entity, With<Stroll>>().iter(&self.world).collect();
        let mut n = 0;
        for e in ids {
            let (pos, body, c, s, sp) = (self.world.get::<Pos>(e).unwrap().0, *self.world.get::<Body>(e).unwrap(), *self.world.get::<Contact>(e).unwrap(), *self.world.get::<Stroll>(e).unwrap(), *self.world.get::<NavParams>(e).unwrap());
            let reach = self.world.get::<Nav>(e).unwrap().reach;
            let pm = PathMob { position: pos, width: body.width, height: body.height, on_ground: c.on_ground, in_water: false, max_up_step: body.step_height, max_fall_distance: comfortable_fall_distance(0.0), malus: sp.malus, can_float: sp.can_float, can_open_doors: sp.can_open_doors, can_pass_doors: sp.can_pass_doors, can_walk_over_fences: sp.can_walk_over_fences, stands_on_lava: false };
            let target = [s.tx.ifloor(), s.ty, s.tz.ifloor()];
            let max_len = sp.follow_range.max(16.0);
            std::hint::black_box(Finder::new(mw, &pm, Evaluator::Walk).find(&[target], max_len, reach, (max_len * 16.0).ifloor()));
            n += 1;
        }
        n
    }
    /// 3: the paths are dropped; what the other systems do to the caches between two searches is `travel` run once.
    pub fn bench_pathfind_clear(&mut self, other_work: bool) {
        let ids: Vec<Entity> = self.world.query_filtered::<Entity, With<Stroll>>().iter(&self.world).collect();
        for &e in &ids {
            self.world.get_mut::<Nav>(e).unwrap().path = None;
            self.world.entity_mut(e).remove::<(Following, PathEnded, NeedsPath)>();
        }
        if other_work {
            self.world.run_system_cached(travel).unwrap();
        }
    }
    /// Turns the simulated chunks on (`zone.rs`): only the chunks within `radius` chunks of a player tick, the creatures of the
    /// others sleep (at the next tick for the players set later).
    pub fn set_simulation(&mut self, radius: i32) {
        crate::zone::enable(&mut self.world, radius);
    }
    /// Whether chunk `(cx, cz)` ticks (always, with the zone off).
    pub fn chunk_simulated(&self, cx: i32, cz: i32) -> bool {
        self.world.resource::<Index>().zone.chunk_simulated(cx, cz)
    }
    /// The players of the herd's world (their positions; cows within 32 blocks stay active).
    pub fn set_players(&mut self, players: Vec<(f64, f64)>) {
        let mut p = self.world.resource_mut::<Players>();
        p.list = players;
        p.dirty = true;
        p.zone_dirty = true;
    }
    /// The players as `Online` knows them (ids from 1, at y = 1, nothing in hand), unkillable: for the hunting benches.
    pub fn set_online(&mut self, players: &[(f64, f64)]) {
        self.world.resource_mut::<crate::hostile::PlayerState>().immortal = true;
        for (i, p) in players.iter().enumerate() {
            self.world.resource_mut::<crate::goals::Online>().set(crate::goals::Player { id: i as u32 + 1, x: p.0, y: 1.0, z: p.1, main: 0, off: 0, ..Default::default() });
        }
    }
    /// Day or night for the monsters that burn in daylight (`monsters_burn`): every one of them looks at the sun again.
    pub fn set_day(&mut self, day: bool) {
        self.world.resource_mut::<crate::hostile::Daylight>().0 = day;
        let all: Vec<Entity> = self.world.query_filtered::<Entity, With<crate::hostile::BurnsInDay>>().iter(&self.world).collect();
        self.world.resource_mut::<crate::hostile::SunDirty>().0.extend(all);
    }
    /// The push mode (`Config.ordered_push`): the `Mob`'s order for the animals that overlap another, or the batch.
    pub fn set_ordered_push(&mut self, on: bool) {
        self.world.resource_mut::<Config>().ordered_push = on;
        if on {
            // The animals that exist are put in the spatial index at once (a new animal enters it after its first tick).
            self.world.run_system_cached(reindex).unwrap();
            // Which of the animals that exist overlap another is not known yet: the first pass looks at all of them (the ones
            // that overlap nothing leave the set at once).
            let all: Vec<Entity> = self.world.query_filtered::<Entity, With<Bounds>>().iter(&self.world).collect();
            for e in all {
                self.world.entity_mut(e).insert(Overlapping);
            }
        }
    }
    /// Fills the slab with water from y = 1 to y = `top` (a lake for the fish), taken at the next tick.
    pub fn fill_water(&mut self, top: i32) {
        let half = self.world.resource::<Level>().half;
        for x in -half + 1..half {
            for z in -half + 1..half {
                for y in 1..=top {
                    self.set_block(x, y, z, "minecraft:water");
                }
            }
        }
    }
    /// A block placed or removed (`name`: `minecraft:stone`, `minecraft:air`, `minecraft:lava`...): taken at the next step.
    pub fn set_block(&mut self, x: i32, y: i32, z: i32, name: &'static str) {
        self.world.resource_mut::<BlockChanges>().0.push((x, y, z, name));
    }
    /// A block placed with properties (`minecraft:sweet_berry_bush`, `&[("age", "3")]`): taken at the next step.
    pub fn set_block_state(&mut self, x: i32, y: i32, z: i32, name: &'static str, props: &'static [(&'static str, &'static str)]) {
        self.world.resource_mut::<StateChanges>().0.push((x, y, z, name, props));
    }
    /// A hit by an entity at `from` (x, z): the damage and the knockback away from it are one event.
    /// A hit by player `player` (of `Online`): from where it stands.
    pub fn hit_by(&mut self, e: Entity, amount: f32, player: u32) {
        let p = self.world.resource::<crate::goals::Online>().get(player).expect("an online player");
        self.world.resource_mut::<DamageQueue>().add_by(e, amount, (p.x, p.z), player);
    }
    pub fn hit(&mut self, e: Entity, amount: f32, from: (f64, f64)) {
        self.world.resource_mut::<DamageQueue>().add_from(e, amount, Cause::Entity, from);
    }
    /// Puts the cow at `at`: its box follows, and the events that can put it into blocks or a fluid are raised.
    pub fn teleport(&mut self, e: Entity, at: Vec3) {
        teleport(&mut self.world, e, at);
    }
    /// The block writes, sounds and particles written since the last call (`host_out`), for the host to apply.
    pub fn drain_host_out(&mut self) -> Vec<crate::host_out::HostEvent> {
        self.world.resource_mut::<crate::goals::Events>().host.drain().collect()
    }
    pub fn tick(&mut self) {
        self.schedule.run(&mut self.world);
        // The messages of removed components (`RemovedComponents`) are dropped at the end of a tick, as the game's tick must do: without it they pile up without bound.
        self.world.clear_trackers();
    }
    /// The cow grows up in `ticks` ticks (it is a baby until then: the baby's size, one wheel entry).
    pub fn make_baby(&mut self, e: Entity, ticks: u64) {
        let now = now_of(&self.world, e);
        let sizes = *self.world.get::<Sizes>(e).unwrap();
        let p = self.world.get::<Pos>(e).unwrap().0;
        let mut b = self.world.get_mut::<Body>(e).unwrap();
        b.width = sizes.baby.0;
        b.height = sizes.baby.1;
        self.world.get_mut::<Bounds>(e).unwrap().0 = box_at(p, sizes.baby.0, sizes.baby.1);
        self.world.entity_mut(e).insert(GrowAt(now + ticks));
        arm_at(&mut self.world, e, now + ticks, Due::Grow);
    }
    /// One tick with the wall time of each system added to `acc` (seconds), in the schedule's order.
    pub fn tick_timed(&mut self, acc: &mut [f64; 12]) {
        macro_rules! t {
            ($i:expr, $s:expr) => {
                let t0 = std::time::Instant::now();
                self.world.run_system_cached($s).unwrap();
                acc[$i] += t0.elapsed().as_secs_f64();
            };
        }
        t!(0, due);
        t!(1, player_near);
        t!(2, pathfind);
        t!(3, follow);
        t!(4, move_control);
        t!(5, travel);
        t!(6, reindex);
        t!(7, push);
        t!(8, path_ended);
        t!(9, apply_pushes);
        t!(10, hazard_tick);
        t!(11, apply_damage);
        self.world.clear_trackers();
    }
    pub const SYSTEMS: [&'static str; 12] = ["due", "player_near", "pathfind", "follow", "move_control", "travel", "reindex", "push", "path_ended", "apply_pushes", "hazard_tick", "apply_damage"];
    /// (cows with no active module, cows that follow a path, cows whose physics is active, total)
    pub fn counts(&mut self) -> (usize, usize, usize, usize) {
        let following = self.world.query_filtered::<(), With<Following>>().iter(&self.world).count();
        let moving = self.world.query_filtered::<(), With<Moving>>().iter(&self.world).count();
        // Only what has a place: the world also holds entities of the framework (the registered components), which are not animals.
        let idle = self.world.query_filtered::<(), (With<Pos>, Without<Moving>, Without<Following>)>().iter(&self.world).count();
        let total = self.world.query::<&Pos>().iter(&self.world).count();
        (idle, following, moving, total)
    }
}

/// The resources and the schedule-side tables of a herd: the level (its blocks, read by the systems), the species, the nbt declarations,
/// the config and the spatial index. The herd builder and the host world (`bridge/assemble.rs`) build them the same way.
pub(crate) fn assemble_resources(world: &mut World, level: Level, sleep: bool, parallel: bool) {
    let half = level.half;
    world.insert_resource(Hazards::build(&level));
    world.insert_resource(DamageQueue::default());
    world.insert_resource(HurtCauses::default());
    world.insert_resource(BlockChanges::default());
    world.insert_resource(StateChanges::default());
    world.insert_resource(HostWrites::default());
    world.insert_resource(level);
    world.insert_resource(species_table());
    crate::type_tags::register(world, crate::effects::TYPED_TAGS);
    let tags = [crate::nbt::Field {
        key: "Tags".into(),
        owner: None,
        accepts: crate::nbt::accept::any,
        write: crate::nbt_tag::write,
        read: crate::nbt_tag::read,
    }];
    crate::nbt::begin_declarations(world);
    for fields in [crate::nbt::COMMON, crate::nbt_fields::common::FIELDS, crate::nbt_fields::hanging::FIELDS, crate::nbt_fields::common::CREATORS, crate::credit::CREATORS, crate::passive::FIELDS, crate::dragon::save::FIELDS, crate::behaviours::trade_offers::FIELDS, crate::behaviours::variant::FIELDS,crate::behaviours::swallow_hold::FIELDS, crate::nbt_fields::owners::FIELDS, crate::nbt_fields::falling::FIELDS, crate::nbt_fields::effect_cloud::FIELDS, crate::nbt_fields::breath_cloud::FIELDS,crate::nbt_fields::creatures::FIELDS, crate::nbt_fields::mount_inventory::FIELDS, crate::nbt_fields::item_store::FIELDS, crate::firework_burst::FIELDS, crate::cart_state::FIELDS, crate::nbt_fields::living_state::FIELDS, crate::nbt_fields::living_state::CREATORS, crate::behaviours::stew_flower::CREATORS, crate::dispense::CREATORS, crate::nbt_fields::creatures_more::FIELDS, crate::nbt_fields::creatures_more::CREATORS, crate::coating::FIELDS, crate::nbt_fields::rides::FIELDS, crate::nbt_fields::things::FIELDS, crate::behaviours::food_stock::FIELDS, crate::behaviours::bed::FIELDS, crate::nbt_fields::villager_memories::FIELDS, &tags[..]] {
        crate::nbt::register(world, fields);
    }
    crate::nbt::register_creators(world, crate::nbt_fields::rides::CREATORS);
    crate::nbt::register_creators(world, crate::nbt_fields::common::CREATORS);
    crate::nbt::register_creators(world, crate::nbt_fields::creatures_more::CREATORS);
    crate::nbt::register_creators(world, crate::nbt_fields::living_state::CREATORS);
    crate::nbt::register_creators(world, crate::dispense::CREATORS);
    crate::nbt::register_creators(world, crate::behaviours::stew_flower::CREATORS);
    crate::nbt::register_creators(world, crate::credit::CREATORS);
    crate::cart_state::register_nbt(world);
    crate::boat_state::register_nbt(world);
    // Every species, then the passive types, then the sorts with a constructor (which replaces the species' recipe of their id).
    for sp in species_table().0 {
        crate::nbt::declare(world, sp.name, crate::nbt::Recipe { saved: sp.saved, build: crate::nbt::Build::Species });
    }
    for id in crate::passive::kinds() {
        crate::nbt::declare(world, id, crate::nbt::Recipe { saved: true, build: crate::nbt::Build::Passive });
    }
    for &(id, b) in crate::nbt_fields::things::builders() {
        crate::nbt::declare(world, id, crate::nbt::Recipe { saved: true, build: crate::nbt::Build::With(b) });
    }
    for &(id, _) in crate::projectile::KINDS {
        // A kind with a recipe of its own (the trident's builder) keeps it.
        if crate::projectile::UNSAVED.contains(&id) {
            continue;
        }
        crate::nbt::declare(world, id, crate::nbt::Recipe { saved: true, build: crate::nbt::Build::With(crate::projectile::build) });
    }
    crate::nbt::end_declarations(world);
    world.insert_resource(Config { sleep, parallel, ordered_push: false, keep_touching: std::env::var_os("TOUCHING").is_some(), path_budget: std::env::var("PATH_BUDGET").ok().and_then(|v| v.parse().ok()).unwrap_or(0), push_form: if std::env::var("PUSH_FORM").as_deref() == Ok("b") { 1 } else { 0 }, push_k: std::env::var("PUSH_K").ok().and_then(|v| v.parse().ok()).unwrap_or(1.0) });
    world.insert_resource(OrderedStats::default());
    world.insert_resource(Clock::default());
    world.insert_resource(Players::default());
    world.insert_resource(Wheel::default());
    crate::goals::insert_resources(world);
    crate::inside::insert_resources(world);
    crate::inside_effects::insert_resources(world);
    crate::inside_more::insert_resources(world);
    crate::hostile::insert_resources(world);
    world.insert_resource(Index::new(-f64::from(half), (f64::from(2 * half) / CELL) as usize + 2));
}

/// The natural spawning of a level, as the production level and the harness install it: the biome tables and zoom of `mw` with `seed`, the
/// rules (`spawn_mobs`, `monsters`) as given, and the structures' host when there is one.
pub fn install_spawning(w: &mut World, mw: &MobWorld<'_>, seed: i64, rules: (bool, bool), structures: Option<crate::spawn_structures::StructureHost>) {
    let species = species_table();
    let tables = mw.biomes.list.iter().map(|info| crate::spawning::Table::from_biome(info, &species)).collect();
    crate::spawning::ensure(w);
    w.insert_resource(crate::spawning::Spawner::new(tables, crate::spawn_biome::zoom_of(mw), seed));
    if let Some(host) = structures {
        w.insert_resource(host);
    }
    let env = &mut *w.get_resource_or_insert_with(crate::spawn_env::SpawnEnv::default);
    (env.spawn_mobs, env.monsters, env.changed) = (rules.0, rules.1, true);
}
