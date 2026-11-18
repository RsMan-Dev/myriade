//! A cow as data components and batch systems (palier 1): the stroll goal (a geometric wait with the law of
//! `nextInt(60) == 0` each tick, then `LandRandomPos` 10 x 7), the move control, `LivingEntity.travel` on the
//! ground with Horizon's real block collision sweep (`SceneCollisions::resolve_in_border_with`), and sleeping.
//! Species data (size, step, speed, stroll range) are component values, not branches.
use bevy_ecs::{
    prelude::*,
    schedule::{Schedule, ScheduleLabel, SingleThreadedExecutor},
};
use horizon_client_lib::mobs::{
    Mob, MobWorld,
    nav::Navigation,
    path::{Evaluator, Finder, Path, PathMob, PATH_TYPES},
};
use horizon_client_render::{chunk::Chunk, collisions::SceneCollisions};
use horizon_core::geometry::{Aabb, Vec3, trig};
use std::sync::Arc;

// ---- components: data only -------------------------------------------------------------------------------
#[derive(Component, Clone, Copy, Debug)]
pub struct Pos(pub Vec3);
#[derive(Component, Clone, Copy, Debug)]
pub struct Vel(pub Vec3);
#[derive(Component, Clone, Copy, Debug)]
pub struct Yaw(pub f32);
/// The species' size and walking data.
#[derive(Component, Clone, Copy, Debug)]
pub struct Body {
    pub width: f32,
    pub height: f32,
    pub step_height: f32,
    pub gravity: f64,
    /// The movement speed attribute.
    pub speed: f64,
}
/// The world box of the cow: persistent data, translated by the move, changed by growth; nothing rebuilds it.
#[derive(Component, Clone, Copy, Debug)]
pub struct Bounds(pub Aabb);
/// The column of the spatial index the cow is listed in (`u32::MAX`: not yet).
#[derive(Component, Clone, Copy, Debug)]
pub struct Cell(pub u32);
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Contact {
    pub on_ground: bool,
    pub horizontal: bool,
    pub fall_distance: f64,
}
/// What the move control left for the travel: the forward input and the speed it scales by.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Drive {
    pub zza: f32,
    pub speed: f32,
    /// The jump input (a goal sets it: `FloatGoal` keeps it while the cow is in deep water).
    pub jumping: bool,
}
/// `MoveControl`: the spot to walk to this tick.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct MoveTo {
    pub active: bool,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub speed_modifier: f64,
}
/// `RandomStrollGoal`: whether a walk is on, where to, whether an attempt is waiting in the timer wheel.
#[derive(Component, Clone, Copy, Debug)]
pub struct Stroll {
    pub walking: bool,
    /// A stroll attempt is in the timer wheel.
    pub scheduled: bool,
    pub tx: f64,
    pub tz: f64,
    pub horizontal_range: i32,
    pub vertical_range: i32,
    /// The attempt's chance is 1 in this (`reducedTickDelay(120)`).
    pub interval: i32,
}
#[derive(Component, Clone, Copy, Debug)]
pub struct Rng(pub u64);
/// `PathNavigation`: the path being followed.
#[derive(Component, Clone, Debug)]
pub struct Nav {
    pub path: Option<Path>,
    /// `PathNavigation.doStuckDetection`: where the cow was 100 ticks ago.
    pub last_check: u64,
    pub last_pos: Vec3,
    /// The speed modifier of the goal that asked for the path.
    pub speed: f64,
}
impl Default for Nav {
    fn default() -> Self {
        Self { path: None, last_check: 0, last_pos: Vec3::ZERO, speed: 1.0 }
    }
}
/// The size of an adult and of a baby of the species (data, not code).
#[derive(Component, Clone, Copy, Debug)]
pub struct Sizes {
    pub adult: (f32, f32),
    pub baby: (f32, f32),
}
/// A path was asked for (the stroll picked a spot): `pathfind` visits only these.
#[derive(Component, Clone, Copy, Debug)]
#[cfg_attr(feature = "sparse", component(storage = "SparseSet"))]
pub struct NeedsPath;
/// The walk of the cow is over (path done, blocked, or none found): `path_ended` arms its next stroll attempt.
#[derive(Component, Clone, Copy, Debug)]
pub struct PathEnded;
/// The cow follows a path: `follow` and `move_control` visit only these.
#[derive(Component, Clone, Copy, Debug)]
#[cfg_attr(feature = "sparse", component(storage = "SparseSet"))]
pub struct Following;
/// The physics of the cow is active (it moves, falls, or was just pushed): `travel` visits only these, and takes
/// the marker off when the cow is back at rest.
#[derive(Component, Clone, Copy, Debug)]
#[cfg_attr(feature = "sparse", component(storage = "SparseSet"))]
pub struct Moving;
/// `Mob.noActionTime` as a deadline: the stroll goal is refused from this tick on (100 ticks after a player was
/// last within 32 blocks, or the cow was hurt).
#[derive(Component, Clone, Copy, Debug)]
pub struct ActiveUntil(pub u64);
/// The players of the bench: positions (x, z). `dirty` is the event: a player moved, came or left.
#[derive(Resource, Default)]
pub struct Players {
    pub list: Vec<(f64, f64)>,
    prev: Vec<(f64, f64)>,
    pub dirty: bool,
}
/// `EntityFluidInteraction`: the fluid heights the box has, and whether the feet and the eyes are in them.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct FluidState {
    pub water: f64,
    pub lava: f64,
    pub in_water: bool,
    pub in_lava: bool,
    pub eye_in_water: bool,
    pub eye_in_lava: bool,
}
/// The fluid module is active: the box overlaps a column with water or lava (`fluid_update` visits only these).
#[derive(Component, Clone, Copy, Debug)]
pub struct InFluid;
/// `Entity.getAirSupply`.
#[derive(Component, Clone, Copy, Debug)]
pub struct Air(pub i32);
/// The air is below its maximum: the breathing module ticks until it is full again.
#[derive(Component, Clone, Copy, Debug)]
pub struct AirLow;
/// Ask `wall_check` to look whether the cow's head is in a block (a spawn, a teleport, a growth: the events that
/// can put a box into blocks).
#[derive(Component, Clone, Copy, Debug)]
pub struct Recheck;
/// The head of the cow is in a suffocating block: it hurts every tick until that stops.
#[derive(Component, Clone, Copy, Debug)]
pub struct InWall;
/// What a hit pushes: the cows to knock back, from where (`LivingEntity.knockback`, strength 0.4).
#[derive(Resource, Default)]
pub struct Knockbacks(pub Vec<(Entity, f64, f64)>);
/// `LivingEntity.health`.
#[derive(Component, Clone, Copy, Debug)]
pub struct Health(pub f32);
/// The last hurt: the tick and the amount (`invulnerableTime`: for 10 ticks after, only a larger hit counts, by the
/// difference).
#[derive(Component, Clone, Copy, Debug)]
pub struct HurtAt {
    pub at: u64,
    pub last: f32,
}
/// On fire until this tick; its damage is in the timer wheel (`Due::Burn`, every 20 ticks).
#[derive(Component, Clone, Copy, Debug)]
pub struct Burning {
    pub until: u64,
}
/// The hazards under the cow's box (bits of `HAZ_*`): present only while it overlaps one.
#[derive(Component, Clone, Copy, Debug)]
pub struct Hazard(pub u8);
/// The ticks `ticks_frozen` has counted in powder snow.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Frozen(pub i32);
pub const HAZ_LAVA: u8 = 1;
pub const HAZ_FIRE: u8 = 2;
pub const HAZ_WATER: u8 = 4;
pub const HAZ_SNOW: u8 = 8;
pub const HAZ_CACTUS: u8 = 16;
/// The damage waiting to be applied (what hurts a cow only queues it: a command).
#[derive(Resource, Default)]
pub struct DamageQueue(pub Vec<(Entity, f32, Cause)>);
/// What hurt (`DamageType`): `Panic` reads it (`#panic_environmental_causes`: cactus, freeze, hot_floor, in_fire, lava,
/// lightning_bolt, on_fire).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cause {
    Cactus,
    InFire,
    OnFire,
    Lava,
    Freeze,
    Fall,
    Drown,
    InWall,
    /// A hit by an entity (a player): `Panic` runs from it by the attacker rule, not the environmental list.
    Entity,
}
/// The causes of the hurts of this step, in the order of `Hurts` (read by the goals that depend on the damage type).
#[derive(Resource, Default)]
pub struct HurtCauses(pub Vec<(Entity, Cause)>);
/// The hazard bits of each block column (a static world here; a block change would update its column: the event).
#[derive(Resource, Default)]
pub struct Hazards {
    origin: i32,
    width: usize,
    bits: Vec<u8>,
}
impl Hazards {
    pub fn build(level: &Level) -> Self {
        let mw = level.mw.0;
        let (origin, width) = (-level.half, (2 * level.half + 1) as usize);
        let mut bits = vec![0u8; width * width];
        for x in 0..width {
            for z in 0..width {
                let mut b = 0;
                for y in 0..=1 {
                    let state = mw.world.block(x as i32 + origin, y, z as i32 + origin);
                    match &*mw.facts.get(state).name {
                        "minecraft:lava" => b |= HAZ_LAVA,
                        "minecraft:fire" => b |= HAZ_FIRE,
                        "minecraft:water" => b |= HAZ_WATER,
                        "minecraft:powder_snow" => b |= HAZ_SNOW,
                        "minecraft:cactus" => b |= HAZ_CACTUS,
                        _ => {}
                    }
                }
                bits[z * width + x] = b;
            }
        }
        Self { origin, width, bits }
    }
    fn at(&self, x: i32, z: i32) -> u8 {
        let (cx, cz) = (x - self.origin, z - self.origin);
        if cx < 0 || cz < 0 || cx as usize >= self.width || cz as usize >= self.width {
            0
        } else {
            self.bits[cz as usize * self.width + cx as usize]
        }
    }
    /// The hazards of the columns a box covers.
    pub fn of_box(&self, b: &Aabb) -> u8 {
        let (mn, mx) = (b.min_position(), b.max_position());
        let e = 1.0e-7;
        let mut bits = 0;
        for x in mn.x.floor() as i32..=(mx.x - e).floor() as i32 {
            for z in mn.z.floor() as i32..=(mx.z - e).floor() as i32 {
                bits |= self.at(x, z);
            }
        }
        bits
    }
}
/// A baby grows up at this tick (it is in the timer wheel then; nothing counts its age every tick).
#[derive(Component, Clone, Copy, Debug)]
pub struct GrowAt(pub u64);

impl Rng {
    pub fn next_u32(&mut self) -> u32 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        (self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 32) as u32
    }
    pub fn next_int(&mut self, bound: i32) -> i32 {
        (self.next_u32() % bound as u32) as i32
    }
    pub fn next_f64(&mut self) -> f64 {
        f64::from(self.next_u32()) / (f64::from(u32::MAX) + 1.0)
    }
}

// ---- resources -------------------------------------------------------------------------------------------
/// The tick counter (advanced by `due`).
#[derive(Resource, Default)]
pub struct Clock(pub u64);
#[derive(Clone, Copy, Debug)]
pub enum Due {
    Stroll,
    Grow,
    Goal(crate::goals::Kind),
    Burn,
}
const WHEEL: usize = 4096;
/// A timer wheel: what is due at each tick, so that a sleeping or growing cow costs nothing until then.
#[derive(Resource)]
pub struct Wheel {
    slots: Vec<Vec<(Entity, Due)>>,
}
impl Default for Wheel {
    fn default() -> Self {
        Self { slots: vec![Vec::new(); WHEEL] }
    }
}
impl Wheel {
    pub fn push(&mut self, at: u64, e: Entity, d: Due) {
        self.slots[(at % WHEEL as u64) as usize].push((e, d));
    }
}
const CELL: f64 = 2.0;
/// The spatial index: the entities by 2-block column of their box's centre, a resource. It is changed only when a
/// box changes column (`reindex`, after the move): a cow that does not move costs it nothing.
#[derive(Resource, Default)]
pub struct Index {
    origin: f64,
    width: usize,
    cells: Vec<Vec<Entity>>,
    stamp: Vec<u32>,
}
impl Index {
    pub fn new(origin: f64, width: usize) -> Self {
        Self { origin, width, cells: vec![Vec::new(); width * width], stamp: vec![0; width * width] }
    }
    fn column(&self, x: f64) -> i64 {
        ((x - self.origin) / CELL).floor() as i64
    }
    pub fn cell_of(&self, x: f64, z: f64) -> u32 {
        let cx = self.column(x).clamp(0, self.width as i64 - 1) as usize;
        let cz = self.column(z).clamp(0, self.width as i64 - 1) as usize;
        (cz * self.width + cx) as u32
    }
    pub fn insert(&mut self, e: Entity, cell: u32) {
        self.cells[cell as usize].push(e);
    }
    pub fn remove(&mut self, e: Entity, cell: u32) {
        let list = &mut self.cells[cell as usize];
        if let Some(i) = list.iter().position(|&o| o == e) {
            list.swap_remove(i);
        }
    }
    /// Calls `f` for the entities in the columns within `r` blocks of `(x, z)` (a box is at most 2 blocks wide).
    pub fn for_radius(&self, x: f64, z: f64, r: f64, mut f: impl FnMut(Entity)) {
        let w = self.width as i64;
        for zz in self.column(z - r - 1.0).max(0)..=self.column(z + r + 1.0).min(w - 1) {
            for xx in self.column(x - r - 1.0).max(0)..=self.column(x + r + 1.0).min(w - 1) {
                for &e in &self.cells[(zz * w + xx) as usize] {
                    f(e);
                }
            }
        }
    }
    /// Calls `f` for the entities in the columns around `(x, z)` (3 x 3, a box is at most 2 blocks wide).
    pub fn for_near(&self, x: f64, z: f64, mut f: impl FnMut(Entity)) {
        let (cx, cz) = (self.column(x), self.column(z));
        let w = self.width as i64;
        for zz in (cz - 1).max(0)..=(cz + 1).min(w - 1) {
            for xx in (cx - 1).max(0)..=(cx + 1).min(w - 1) {
                for &e in &self.cells[(zz * w + xx) as usize] {
                    f(e);
                }
            }
        }
    }
}

/// The mob world of the bench (the blocks, the registries, the data), built once and kept for the process:
/// shared read-only by the systems (the kernels it holds keep their caches in thread locals).
#[derive(Clone, Copy)]
pub struct Mw(pub &'static MobWorld<'static>);
// SAFETY of the prototype: the world is only read once built, and every cache behind it is thread local.
unsafe impl Send for Mw {}
unsafe impl Sync for Mw {}
#[derive(Resource, Clone, Copy)]
pub struct Level {
    pub mw: Mw,
    /// The walls are at |x| or |z| = `half`.
    pub half: i32,
}
#[derive(Resource, Clone, Copy)]
pub struct Config {
    pub sleep: bool,
    pub parallel: bool,
}
/// The species' navigation data, read once from a real `Mob` of the type.
#[derive(Resource, Clone, Copy)]
pub struct Species {
    pub malus: [Option<f32>; PATH_TYPES],
    pub can_float: bool,
    pub can_open_doors: bool,
    pub can_pass_doors: bool,
    pub can_walk_over_fences: bool,
    pub follow_range: f32,
    pub max_health: f32,
    pub water_slowdown: f32,
    pub water_efficiency: f32,
    pub eye_height: f64,
    pub breathes_water: bool,
}
#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
pub struct CowTick;

/// A walled stone slab of `half` blocks each way from the origin, on the prepared vanilla data.
pub fn build_level(half: i32, patches: bool) -> (Level, Species, Arc<horizon_client_lib::mobs::MobType>) {
    let build = move |blocks: &mut horizon_client_render::world::World, registry: &horizon_world::registry::BlockRegistry| {
        let stone = registry.default_state("minecraft:stone").unwrap();
        let chunks = (half + 15) / 16 + 1;
        for cx in -chunks..chunks {
            for cz in -chunks..chunks {
                blocks.chunks.insert((cx, cz), Arc::new(Chunk::empty(cx, cz)));
            }
        }
        for x in -half..=half {
            for z in -half..=half {
                blocks.set(x, -2, z, stone).unwrap();
                blocks.set(x, -1, z, stone).unwrap();
                blocks.set(x, 0, z, stone).unwrap();
                if x.abs() == half || z.abs() == half {
                    blocks.set(x, 1, z, stone).unwrap();
                    blocks.set(x, 2, z, stone).unwrap();
                }
            }
        }
        if patches {
            // 3 x 3 hazards every 48 blocks: lava, a pond, powder snow (in the floor), fire and cactus (on it).
            let put = |blocks: &mut horizon_client_render::world::World, name: &str, x0: i32, z0: i32, y: i32| {
                let state = registry.default_state(name).unwrap();
                for x in x0..x0 + 3 {
                    for z in z0..z0 + 3 {
                        blocks.set(x, y, z, state).unwrap();
                    }
                }
            };
            let mut gx = -half + 8;
            while gx + 40 < half {
                let mut gz = -half + 8;
                while gz + 40 < half {
                    put(blocks, "minecraft:lava", gx, gz, 0);
                    put(blocks, "minecraft:water", gx + 8, gz, 0);
                    put(blocks, "minecraft:water", gx + 8, gz, -1);
                    put(blocks, "minecraft:powder_snow", gx + 16, gz, 0);
                    put(blocks, "minecraft:fire", gx, gz + 8, 1);
                    put(blocks, "minecraft:cactus", gx + 8, gz + 8, 1);
                    gz += 48;
                }
                gx += 48;
            }
        }
    };
    let (mw, types) =
        horizon_client_lib::mobs::world::leaked_test_world(true, build).expect("HORIZON_TEST_RESOURCES");
    let kind = Arc::new(types.types["minecraft:cow"].clone());
    let mob = Mob::new(1, kind.clone(), Vec3::new(0.5, 1.0, 0.5), 0.0);
    let species = Species {
        malus: mob.ai.malus,
        can_float: mob.ai.nav.can_float,
        can_open_doors: mob.ai.nav.can_open_doors,
        can_pass_doors: mob.ai.nav.can_pass_doors,
        can_walk_over_fences: mob.ai.nav.can_walk_over_fences,
        follow_range: mob.attrs.follow_range as f32,
        max_health: mob.health,
        water_slowdown: kind.water_slowdown,
        water_efficiency: kind.water_efficiency as f32,
        eye_height: kind.eye_height,
        breathes_water: kind.flags.breathes_water,
    };
    (Level { mw: Mw(mw), half }, species, kind)
}

/// The box of a cow of this size standing at `p`.
pub fn box_at(p: Vec3, width: f32, height: f32) -> Aabb {
    let (hw, h) = (f64::from(width) / 2.0, f64::from(height));
    Aabb::new(Vec3::new(p.x - hw, p.y, p.z - hw), Vec3::new(p.x + hw, p.y + h, p.z + hw))
}
pub fn cow_bundle(x: f64, z: f64, seed: u64) -> impl Bundle {
    let mut rng = Rng(seed | 1);
    (
        Pos(Vec3::new(x, 1.0, z)),
        Vel(Vec3::ZERO),
        Yaw((rng.next_u32() % 360) as f32),
        Body { width: 0.9, height: 1.4, step_height: 0.6, gravity: 0.08, speed: 0.2 },
        Bounds(box_at(Vec3::new(x, 1.0, z), 0.9, 1.4)),
        Cell(u32::MAX),
        Contact { on_ground: true, horizontal: false, fall_distance: 0.0 },
        Drive::default(),
        (
        Sizes { adult: (0.9, 1.4), baby: (0.45, 0.7) },
        MoveTo::default(),
        Nav::default(),
        ActiveUntil(100),
        Stroll { walking: false, scheduled: false, tx: 0.0, tz: 0.0, horizontal_range: 10, vertical_range: 7, interval: 60 },
        (Moving, crate::goals::cow_goals()),
        Health(10.0),
        HurtAt { at: 0, last: 0.0 },
        Frozen(0),
        (FluidState::default(), Air(300), Recheck),
        rng,
        ),
    )
}

/// The number of failed attempts before the first success of a 1-in-`n` draw each tick (the same law as
/// drawing `nextInt(n) == 0` every tick, sampled once).
/// The goal selector asks a goal that is not running whether it can start every OTHER tick: the wait is twice the
/// number of failed asks.
pub fn idle_wait(rng: &mut Rng, n: i32) -> i32 {
    2 * geometric(rng, n)
}
pub fn geometric(rng: &mut Rng, n: i32) -> i32 {
    let u = rng.next_f64().max(1.0e-12);
    (u.ln() / (1.0 - 1.0 / f64::from(n)).ln()).floor() as i32
}

pub fn schedule() -> Schedule {
    let mut s = Schedule::new(CowTick);
    s.set_executor(SingleThreadedExecutor::new());
    // The order is explicit: due, index, stroll, pathfind, follow, move control, travel, push, resume, sleep.
    s.add_systems((due, player_near, pathfind, follow, path_ended, move_control, fluid_update, breathing, wall_check, travel, reindex, hazard_tick, wall_damage, push, apply_pushes, apply_damage).chain());
    crate::goals::register(&mut s);
    s
}

// ---- systems ---------------------------------------------------------------------------------------------
/// `RandomStrollGoal.canUse` and `LandRandomPos`: after the wait, 10 tries of a spot within 10 x 7 (the first valid
/// one wins as all the ground weighs the same); the path to it is made by `pathfind`.
/// Arms the stroll attempt `idle_wait` ticks from now (the goal selector asks every other tick, 1 chance in 60).
fn arm_stroll(wheel: &mut Wheel, now: u64, e: Entity, s: &mut Stroll, r: &mut Rng) {
    let at = now + idle_wait(r, s.interval).max(1) as u64;
    wheel.push(at.min(now + WHEEL as u64 - 1), e, Due::Stroll);
    s.scheduled = true;
}
/// `RandomStrollGoal.canUse` and `LandRandomPos`, at the tick the attempt was due: 10 tries of a spot within
/// 10 x 7 (the first valid one wins as all the ground weighs the same); the path to it is made by `pathfind`.
/// Returns whether a spot was picked.
pub(crate) fn pick_spot(level: &Level, pos: &Pos, s: &mut Stroll, r: &mut Rng) -> bool {
    let (h, v) = (s.horizontal_range, s.vertical_range);
    let lim = level.half - 2;
    for _ in 0..10 {
        let dx = r.next_int(2 * h + 1) - h;
        let dy = r.next_int(2 * v + 1) - v;
        let dz = r.next_int(2 * h + 1) - h;
        let x = (pos.0.x + f64::from(dx)).floor() as i32;
        let z = (pos.0.z + f64::from(dz)).floor() as i32;
        // A spot in the air has nothing under it; one in the stone is lifted out of it onto the slab.
        if dy <= 0 && x.abs() < lim && z.abs() < lim {
            s.walking = true;
            s.tx = f64::from(x) + 0.5;
            s.tz = f64::from(z) + 0.5;
            return true;
        }
    }
    false
}

/// `Mob.checkDespawn`'s reset of `noActionTime`, by event: nothing is read while the players stay where they are.
/// When one moves (`Players.dirty`), the cows of the cells its old and new circles cover are tested against the
/// players of the 3 x 3 bins of 32 blocks around them, and only the cows that enter or leave the 32 blocks are
/// touched: an entering cow is active without end (`ActiveUntil(u64::MAX)`) and its stroll attempt is armed if it
/// had none; a leaving cow stays active for 100 ticks more (the `noActionTime` it has to count up to).
pub(crate) fn player_near(
    clock: Res<Clock>,
    mut players: ResMut<Players>,
    mut idx: ResMut<Index>,
    mut wheel: ResMut<Wheel>,
    mut entered: ResMut<crate::goals::Entered>,
    mut active: Query<(&Bounds, &mut ActiveUntil, &mut Stroll, &mut Rng)>,
) {
    if !players.dirty {
        return;
    }
    players.dirty = false;
    const BIN: f64 = 32.0;
    let origin = idx.origin;
    let bins = ((idx.width as f64 * CELL / BIN).ceil() as i64).max(1) + 2;
    let bin = |v: f64| (((v - origin) / BIN).floor() as i64 + 1).clamp(0, bins - 1);
    let new = players.list.clone();
    let old = std::mem::replace(&mut players.prev, new.clone());
    let mut by_bin: Vec<Vec<(f64, f64)>> = vec![Vec::new(); (bins * bins) as usize];
    for &(px, pz) in &new {
        by_bin[(bin(pz) * bins + bin(px)) as usize].push((px, pz));
    }
    let within = |x: f64, z: f64| {
        let (bx, bz) = (bin(x), bin(z));
        (bz - 1..=bz + 1).any(|zz| {
            (bx - 1..=bx + 1).any(|xx| {
                (0..bins).contains(&xx)
                    && (0..bins).contains(&zz)
                    && by_bin[(zz * bins + xx) as usize].iter().any(|&(px, pz)| (x - px).powi(2) + (z - pz).powi(2) < 32.0 * 32.0)
            })
        })
    };
    // The cells either circle covers, each once.
    let tick = clock.0 as u32 + 1;
    let idx = &mut *idx;
    let mut covered: Vec<usize> = Vec::new();
    for &(px, pz) in new.iter().chain(old.iter()) {
        let (x0, x1) = (idx.column(px - 32.0).max(0), idx.column(px + 32.0).min(idx.width as i64 - 1));
        let (z0, z1) = (idx.column(pz - 32.0).max(0), idx.column(pz + 32.0).min(idx.width as i64 - 1));
        for zz in z0..=z1 {
            for xx in x0..=x1 {
                let c = zz as usize * idx.width + xx as usize;
                if idx.stamp[c] != tick {
                    idx.stamp[c] = tick;
                    covered.push(c);
                }
            }
        }
    }
    for c in covered {
        for &e in &idx.cells[c] {
            let Ok((b, mut a, mut s, mut r)) = active.get_mut(e) else { continue };
            let (mn, mx) = (b.0.min_position(), b.0.max_position());
            let now_in = within((mn.x + mx.x) / 2.0, (mn.z + mx.z) / 2.0);
            if now_in && a.0 != u64::MAX {
                a.0 = u64::MAX;
                entered.0.push(e);
                if !s.scheduled && !s.walking {
                    arm_stroll(&mut wheel, clock.0, e, &mut s, &mut r);
                }
            } else if !now_in && a.0 == u64::MAX {
                a.0 = clock.0 + 100;
            }
        }
    }
}

fn comfortable_fall_distance(f: f32) -> i32 {
    (f + 3.0).floor() as i32
}
/// `PathNavigation.createPath` + `moveTo` for the spot a stroll picked: Horizon's real path finder. Only the cows
/// that asked for a path are visited, in parallel when asked; what follows (the next module, the end of the walk)
/// is a command.
#[allow(clippy::type_complexity)]
fn pathfind(
    cfg: Res<Config>,
    clock: Res<Clock>,
    level: Res<Level>,
    sp: Res<Species>,
    par: ParallelCommands,
    mut q: Query<(Entity, &Pos, &Body, &Contact, &mut Stroll, &mut Nav), With<NeedsPath>>,
) {
    let now = clock.0;
    let level = &*level;
    let one = |e: Entity, pos: &Pos, body: &Body, c: &Contact, s: &mut Stroll, nav: &mut Nav| {
        let mw = level.mw.0;
        let pm = PathMob {
            position: pos.0,
            width: body.width,
            height: body.height,
            on_ground: c.on_ground,
            in_water: false,
            max_up_step: body.step_height,
            max_fall_distance: comfortable_fall_distance(0.0),
            malus: sp.malus,
            can_float: sp.can_float,
            can_open_doors: sp.can_open_doors,
            can_pass_doors: sp.can_pass_doors,
            can_walk_over_fences: sp.can_walk_over_fences,
            stands_on_lava: false,
        };
        let max_len = sp.follow_range;
        let max_visited = (max_len * 16.0).floor() as i32;
        let target = [s.tx.floor() as i32, 1, s.tz.floor() as i32];
        let path = Finder::new(mw, &pm, Evaluator::Walk).find(&[target], max_len, 1, max_visited);
        par.command_scope(|mut cmd| match path {
            Some(p) if !p.nodes.is_empty() => {
                nav.path = Some(p);
                nav.last_check = now;
                nav.last_pos = pos.0;
                cmd.entity(e).remove::<NeedsPath>().insert((Following, Moving));
            }
            _ => {
                nav.path = None;
                s.walking = false;
                cmd.entity(e).remove::<NeedsPath>().insert(PathEnded);
            }
        });
    };
    if cfg.parallel {
        q.par_iter_mut().for_each(|(e, p, b, c, mut s, mut n)| one(e, p, b, c, &mut s, &mut n));
    } else {
        for (e, p, b, c, mut s, mut n) in &mut q {
            one(e, p, b, c, &mut s, &mut n);
        }
    }
}

/// `PathNavigation.tick` -> `followThePath` (ground): reach the waypoint, or cut the corner, then steer the move
/// control at the next node. (The recompute on a block change is not here.) Visits only the cows that follow a path.
#[allow(clippy::type_complexity)]
fn follow(
    cfg: Res<Config>,
    clock: Res<Clock>,
    level: Res<Level>,
    par: ParallelCommands,
    mut q: Query<(Entity, &Pos, &Body, &mut Drive, &mut Stroll, &mut Nav, &mut MoveTo), With<Following>>,
) {
    let now = clock.0;
    let level = &*level;
    let one = |e: Entity, pos: &Pos, body: &Body, drive: &mut Drive, s: &mut Stroll, nav: &mut Nav, m: &mut MoveTo| {
        let mw = level.mw.0;
        let mut end = false;
        match nav.path.as_mut() {
            None => end = true,
            Some(path) => {
                let mob = Vec3::new(pos.0.x, (pos.0.y + 0.5).floor(), pos.0.z);
                let width = body.width;
                let max_distance = if width > 0.75 { width / 2.0 } else { 0.75 - width / 2.0 };
                let n = path.next_node();
                let xd = (pos.0.x - (f64::from(n.x) + 0.5)).abs();
                let yd = (pos.0.y - f64::from(n.y)).abs();
                let zd = (pos.0.z - (f64::from(n.z) + 0.5)).abs();
                let max = f64::from(max_distance);
                let close = xd < max && zd < max && yd < 1.0;
                if close || (Navigation::can_cut_corner(n.kind) && should_target_next_node(path, mob, width)) {
                    path.advance();
                }
                // `doStuckDetection`: every 100 ticks, a cow that has moved less than a quarter of what its speed
                // would carry it gives its path up.
                let mut stuck = false;
                if now - nav.last_check > 100 {
                    let speed = drive.speed;
                    let effective = if speed >= 1.0 { speed } else { speed * speed };
                    let threshold = f64::from(effective * 100.0 * 0.25);
                    let d = (mob.x - nav.last_pos.x).powi(2) + (mob.y - nav.last_pos.y).powi(2) + (mob.z - nav.last_pos.z).powi(2);
                    stuck = d < threshold * threshold;
                    nav.last_check = now;
                    nav.last_pos = mob;
                }
                let path = nav.path.as_ref().unwrap();
                if stuck || path.is_done() {
                    end = true;
                } else {
                    let target = path.entity_pos_at(path.next, width);
                    let (bx, by, bz) = (target.x.floor() as i32, target.y.floor() as i32, target.z.floor() as i32);
                    let y = if mw.facts.is_air(mw.world.block(bx, by.saturating_sub(1), bz)) {
                        target.y
                    } else {
                        mw.floor_level(bx, by, bz)
                    };
                    *m = MoveTo { active: true, x: target.x, y, z: target.z, speed_modifier: nav.speed };
                }
            }
        }
        if end {
            nav.path = None;
            s.walking = false;
            drive.zza = 0.0;
            m.active = false;
            par.command_scope(|mut cmd| {
                cmd.entity(e).remove::<Following>().insert(PathEnded);
            });
        }
    };
    if cfg.parallel {
        q.par_iter_mut().for_each(|(e, p, b, mut d, mut s, mut n, mut m)| one(e, p, b, &mut d, &mut s, &mut n, &mut m));
    } else {
        for (e, p, b, mut d, mut s, mut n, mut m) in &mut q {
            one(e, p, b, &mut d, &mut s, &mut n, &mut m);
        }
    }
}
/// The walks that are over end the goal that asked for them (`goals.rs`), which arms what comes next.
fn path_ended(q: Query<(Entity, &crate::goals::GoalSet), With<PathEnded>>, mut reqs: ResMut<crate::goals::Requests>, mut commands: Commands) {
    for (e, g) in &q {
        crate::goals::path_ended(e, g, &mut reqs);
        commands.entity(e).remove::<PathEnded>();
    }
}
// Vanilla's NaN-sensitive `!(a < b)` form.
#[allow(clippy::neg_cmp_op_on_partial_ord)]
fn should_target_next_node(path: &Path, mob: Vec3, width: f32) -> bool {
    let _ = width;
    if path.next + 1 >= path.nodes.len() {
        return false;
    }
    let n = path.nodes[path.next];
    let current = Vec3::new(f64::from(n.x) + 0.5, f64::from(n.y), f64::from(n.z) + 0.5);
    let d = |a: Vec3, b: Vec3| (a.x - b.x).powi(2) + (a.y - b.y).powi(2) + (a.z - b.z).powi(2);
    if d(mob, current) >= 4.0 {
        return false;
    }
    // `canMoveDirectly` is false for the ground navigation.
    let m = path.nodes[path.next + 1];
    let next = Vec3::new(f64::from(m.x) + 0.5, f64::from(m.y), f64::from(m.z) + 0.5);
    let to_current = Vec3::new(current.x - mob.x, current.y - mob.y, current.z - mob.z);
    let to_next = Vec3::new(next.x - mob.x, next.y - mob.y, next.z - mob.z);
    let (cs, ns) = (to_current.length_squared(), to_next.length_squared());
    if !(ns < cs) && !(cs < 0.5) {
        return false;
    }
    let a = to_current.normalize();
    let b = to_next.normalize();
    a.x * b.x + a.y * b.y + a.z * b.z < 0.0
}

fn wrap(a: f32) -> f32 {
    let mut a = a % 360.0;
    if a >= 180.0 {
        a -= 360.0;
    }
    if a < -180.0 {
        a += 360.0;
    }
    a
}
fn rotlerp(a: f32, b: f32, max: f32) -> f32 {
    let mut r = a + wrap(b - a).clamp(-max, max);
    if r < 0.0 {
        r += 360.0;
    } else if r > 360.0 {
        r -= 360.0;
    }
    r
}
/// `MoveControl.tick`, operation MOVE_TO (the jump over a step is not here: the slab is flat).
fn move_control_one(pos: &Pos, yaw: &mut Yaw, m: &mut MoveTo, d: &mut Drive, body: &Body) {
    const ARRIVED: f64 = 2.500_000_3e-7_f32 as f64;
    if !m.active {
        d.zza = 0.0;
        return;
    }
    m.active = false;
    let (xd, yd, zd) = (m.x - pos.0.x, m.y - pos.0.y, m.z - pos.0.z);
    if xd * xd + yd * yd + zd * zd < ARRIVED {
        d.zza = 0.0;
        return;
    }
    yaw.0 = rotlerp(yaw.0, trig::atan2_degrees(zd, xd) - 90.0, 90.0);
    let speed = (m.speed_modifier * body.speed) as f32;
    d.speed = speed;
    d.zza = speed;
}
fn move_control(cfg: Res<Config>, mut q: Query<(&Pos, &mut Yaw, &mut MoveTo, &mut Drive, &Body), With<Following>>) {
    if cfg.parallel {
        q.par_iter_mut().for_each(|(p, mut y, mut m, mut d, b)| move_control_one(p, &mut y, &mut m, &mut d, b));
    } else {
        for (p, mut y, mut m, mut d, b) in &mut q {
            move_control_one(p, &mut y, &mut m, &mut d, b);
        }
    }
}

/// `LivingEntity.travel` of a cow on the ground (air branch): `Entity.moveRelative`, the real block sweep,
/// gravity and the drags. Block friction is the stone's 0.6.
/// `Entity.moveRelative`.
fn move_relative(vel: &mut Vel, yaw: f32, speed: f32, zza: f32) {
    let input = Vec3::new(0.0, 0.0, f64::from(zza));
    let len_sq = input.x * input.x + input.y * input.y + input.z * input.z;
    if len_sq < 1.0e-7 {
        return;
    }
    let l = len_sq.sqrt();
    let i = if len_sq > 1.0 { Vec3::new(input.x / l, input.y / l, input.z / l) } else { input };
    let i = Vec3::new(i.x * f64::from(speed), i.y * f64::from(speed), i.z * f64::from(speed));
    let angle = f64::from(yaw * trig::DEG_TO_RAD);
    let (s, co) = (f64::from(trig::sin(angle)), f64::from(trig::cos(angle)));
    let u = vel.0;
    vel.0 = Vec3::new(u.x + (i.x * co - i.z * s), u.y + i.y, u.z + (i.z * co + i.x * s));
}
/// `Entity.move`: the sweep over the blocks (Horizon's real kernel), then the collision flags, the landing and the
/// restitution (the vertical velocity is lost after a landing).
fn do_move(level: &Level, pos: &mut Pos, vel: &mut Vel, c: &mut Contact, body: &Body, bounds: &mut Bounds, fall_damage: &mut f32) {
    let wanted = vel.0;
    let p = pos.0;
    let scene = SceneCollisions { world: level.mw.0.world, binding: level.mw.0.collision, subject: None };
    let moved = scene
        .resolve_in_border_with(bounds.0, wanted, body.step_height, c.on_ground, None, &[])
        .unwrap_or(Vec3::ZERO);
    pos.0 = Vec3::new(p.x + moved.x, p.y + moved.y, p.z + moved.z);
    if moved != Vec3::ZERO {
        bounds.0 = bounds.0.move_by(moved);
    }
    let differs = |a: f64, b: f64| (b - a).abs() >= f64::from(1.0e-5_f32);
    c.horizontal = differs(wanted.x, moved.x) || differs(wanted.z, moved.z);
    let vertical = wanted.y != moved.y;
    let below = vertical && wanted.y < 0.0;
    c.on_ground = below;
    if !c.on_ground && moved.y < 0.0 {
        c.fall_distance -= moved.y;
    }
    if c.on_ground {
        // `LivingEntity.causeFallDamage`: the fall beyond 3 blocks, rounded up.
        if c.fall_distance > 3.0 {
            *fall_damage = (c.fall_distance - 3.0).ceil() as f32;
        }
        c.fall_distance = 0.0;
    }
    if vertical && below {
        vel.0.y = 0.0;
    }
}
/// `LivingEntity.travel` of a cow: in the air (on the ground), or in water or lava (`travelInFluid`).
#[allow(clippy::too_many_arguments)]
fn travel_one(
    level: &Level,
    sp: &Species,
    pos: &mut Pos,
    vel: &mut Vel,
    yaw: &Yaw,
    c: &mut Contact,
    d: &Drive,
    body: &Body,
    bounds: &mut Bounds,
    fs: &FluidState,
    jumping: bool,
    fall_damage: &mut f32,
) -> bool {
    let rest_vy = (0.0 - body.gravity) * f64::from(0.98_f32);
    let in_fluid = fs.in_water || fs.in_lava;
    // A cow at its resting state stays as it is: no sweep is made for it.
    if !in_fluid && !jumping && c.on_ground && d.zza == 0.0 && vel.0.x == 0.0 && vel.0.z == 0.0 && vel.0.y == rest_vy {
        return true;
    }
    // `LivingEntity.aiStep`: the tiny motions are dropped.
    let v = vel.0;
    vel.0 = Vec3::new(
        if v.x.abs() < 0.003 { 0.0 } else { v.x },
        if v.y.abs() < 0.003 { 0.0 } else { v.y },
        if v.z.abs() < 0.003 { 0.0 } else { v.z },
    );
    // `LivingEntity.handleJumping`, in a fluid: the jump input lifts a cow whose feet are in water or lava.
    let threshold = if sp.eye_height < 0.4 { 0.0 } else { 0.4 };
    if jumping && in_fluid {
        let deep = if fs.in_lava { fs.lava > threshold } else { fs.water > threshold };
        if deep || !c.on_ground {
            vel.0.y += if sp.can_float { f64::from(0.04_f32) } else { 0.3 };
        }
    }
    if in_fluid {
        let falling = vel.0.y <= 0.0;
        let old_y = pos.0.y;
        if fs.in_water {
            let mut slow = sp.water_slowdown;
            let mut speed = 0.02_f32;
            let mut efficiency = sp.water_efficiency;
            if !c.on_ground {
                efficiency *= 0.5;
            }
            if efficiency > 0.0 {
                slow += (0.546_000_06 - slow) * efficiency;
                speed += (d.speed - speed) * efficiency;
            }
            move_relative(vel, yaw.0, speed, d.zza);
            do_move(level, pos, vel, c, body, bounds, fall_damage);
            let m = vel.0;
            let m = Vec3::new(m.x * f64::from(slow), m.y * f64::from(0.8_f32), m.z * f64::from(slow));
            vel.0 = horizon_client_lib::mobs::living::fluid_falling(body.gravity, falling, false, m);
            c.fall_distance = 0.0;
        } else {
            move_relative(vel, yaw.0, 0.02, d.zza);
            do_move(level, pos, vel, c, body, bounds, fall_damage);
            let v = vel.0;
            if fs.lava <= threshold {
                vel.0 = Vec3::new(v.x * 0.5, v.y * f64::from(0.8_f32), v.z * 0.5);
                vel.0 = horizon_client_lib::mobs::living::fluid_falling(body.gravity, falling, false, vel.0);
            } else {
                vel.0 = Vec3::new(v.x * 0.5, v.y * 0.5, v.z * 0.5);
            }
            if body.gravity != 0.0 {
                vel.0.y -= body.gravity / 4.0;
            }
        }
        // A cow against the shore with free space and no liquid ahead hops out.
        let m = vel.0;
        let ahead = bounds.0.move_by(Vec3::new(m.x, m.y + f64::from(0.6_f32) - pos.0.y + old_y, m.z));
        let mw = level.mw.0;
        if c.horizontal && mw.no_collision(ahead) && !mw.contains_any_liquid(ahead) {
            vel.0.y = f64::from(0.3_f32);
        }
        return false;
    }
    let friction: f32 = if c.on_ground { 0.6 } else { 1.0 };
    let speed = if c.on_ground {
        if friction > 0.6 { d.speed * (0.216_000_02 / (friction * friction * friction)) } else { d.speed }
    } else {
        0.02
    };
    move_relative(vel, yaw.0, speed, d.zza);
    do_move(level, pos, vel, c, body, bounds, fall_damage);
    let movement = vel.0;
    let drag = 0.91_f32;
    let f = friction * drag;
    let y = movement.y - body.gravity;
    vel.0 = Vec3::new(movement.x * f64::from(f), y * f64::from(0.98_f32), movement.z * f64::from(f));
    c.on_ground && d.zza == 0.0 && vel.0.x == 0.0 && vel.0.z == 0.0 && vel.0.y == rest_vy
}
#[allow(clippy::type_complexity)]
fn travel(
    cfg: Res<Config>,
    level: Res<Level>,
    sp: Res<Species>,
    par: ParallelCommands,
    mut q: Query<
        (Entity, &mut Pos, &mut Vel, &Yaw, &mut Contact, &Drive, &Body, &mut Bounds, &FluidState, Has<Following>, Option<&crate::goals::Jump>),
        With<Moving>,
    >,
) {
    let one = |e: Entity, mut p: Mut<Pos>, mut v: Mut<Vel>, y: &Yaw, mut c: Mut<Contact>, d: &Drive, b: &Body, mut bb: Mut<Bounds>, fs: &FluidState, following: bool, jump: Option<&crate::goals::Jump>| {
        let mut fall = 0.0_f32;
        let jumping = jump.is_some_and(|j| j.0);
        let rest = travel_one(&level, &sp, &mut p, &mut v, y, &mut c, d, b, &mut bb, fs, jumping, &mut fall);
        if fall > 0.0 {
            par.command_scope(|mut cmd| {
                cmd.queue(move |w: &mut World| w.resource_mut::<DamageQueue>().0.push((e, fall, Cause::Fall)));
            });
        }
        // At rest and not following a path: the physics of the cow goes inactive until something moves it.
        if rest && !following {
            par.command_scope(|mut cmd| {
                cmd.entity(e).remove::<Moving>();
            });
        }
    };
    if cfg.parallel {
        q.par_iter_mut().for_each(|(e, p, v, y, c, d, b, bb, fs, f, j)| one(e, p, v, y, c, d, b, bb, fs, f, j));
    } else {
        for (e, p, v, y, c, d, b, bb, fs, f, j) in &mut q {
            one(e, p, v, y, c, d, b, bb, fs, f, j);
        }
    }
}

/// `Entity.updateFluidInteraction` (Horizon's real fluid heights and currents), only for the cows whose box overlaps
/// a column with a fluid.
fn fluid_update(
    cfg: Res<Config>,
    level: Res<Level>,
    sp: Res<Species>,
    mut q: Query<(&Pos, &Bounds, &mut Vel, &mut FluidState, &mut Contact), With<InFluid>>,
) {
    let level = &*level;
    let one = |pos: &Pos, b: &Bounds, v: &mut Vel, fs: &mut FluidState, c: &mut Contact| {
        let mw = level.mw.0;
        let f = mw.fluids(b.0, pos.0.y + sp.eye_height);
        *fs = FluidState {
            water: f.water,
            lava: f.lava,
            in_water: f.in_water,
            in_lava: f.in_lava,
            eye_in_water: f.eye_in_water,
            eye_in_lava: f.eye_in_lava,
        };
        if f.in_water || f.in_lava {
            v.0 = mw.pushed_by_currents(b.0, v.0);
        }
        if f.in_water {
            c.fall_distance = 0.0;
        }
    };
    if cfg.parallel {
        q.par_iter_mut().for_each(|(p, b, mut v, mut fs, mut c)| one(p, b, &mut v, &mut fs, &mut c));
    } else {
        for (p, b, mut v, mut fs, mut c) in &mut q {
            one(p, b, &mut v, &mut fs, &mut c);
        }
    }
}

/// `LivingEntity.baseTick`'s air: the cows in a fluid, or with air missing, breathe (Horizon's real `breathe` and
/// `drown`): the air runs down while the eyes are in water, refills by 4 outside, and drowning hurts 2.
#[allow(clippy::type_complexity)]
fn breathing(
    level: Res<Level>,
    sp: Res<Species>,
    mut q: Query<(Entity, &Pos, &FluidState, &mut Air, Has<AirLow>, Has<InFluid>), Or<(With<InFluid>, With<AirLow>)>>,
    mut queue: ResMut<DamageQueue>,
    mut commands: Commands,
) {
    let mw = level.mw.0;
    for (e, pos, fs, mut air, low, in_fluid) in &mut q {
        let submerged = fs.eye_in_water && {
            let (x, y, z) = (pos.0.x.floor() as i32, (pos.0.y + sp.eye_height).floor() as i32, pos.0.z.floor() as i32);
            mw.block_name(x, y, z) != "minecraft:bubble_column"
        };
        let drowns = submerged && !sp.breathes_water;
        let vanilla = horizon_client_lib::breathing::Breath {
            can_breathe: !drowns,
            consume: 1,
            refill: if drowns { 0 } else { 4 },
        };
        let id = e.index().index();
        let breath = horizon_client_lib::breathing::breathe(mw.loop_context, id, "minecraft:cow", vanilla);
        if let Some(d) = horizon_client_lib::breathing::drown(mw.loop_context, id, "minecraft:cow", &mut air.0, 300, breath, true)
            && let Some(damage) = d.damage
        {
            queue.0.push((e, damage, Cause::Drown));
        }
        if air.0 < 300 && !low {
            commands.entity(e).insert(AirLow);
        } else if air.0 >= 300 && low && !in_fluid {
            commands.entity(e).remove::<AirLow>();
        }
    }
}

/// `Mob.baseTick`'s `isInWall` for the cows an event asked about (a spawn, a teleport, a growth): the head in a
/// suffocating block puts the cow in the `InWall` module (1 damage a tick) or takes it out.
fn wall_check(
    level: Res<Level>,
    sp: Res<Species>,
    q: Query<(Entity, &Pos, &Body, Has<InWall>), With<Recheck>>,
    mut commands: Commands,
) {
    use horizon_core::geometry::{ShapeOp, VoxelShape};
    let mw = level.mw.0;
    for (e, pos, body, was) in &q {
        let mut scene = SceneCollisions { world: mw.world, binding: mw.collision, subject: None };
        let inside = horizon_client_lib::mobs::ai::in_wall(
            body.width,
            [pos.0.x, pos.0.y + sp.eye_height, pos.0.z],
            |x, y, z, eye_bounds| {
                let p = [x, y, z];
                let state = mw.world.block(x, y, z);
                if !mw.suffocation.is_suffocating(&mut scene, state, p).unwrap_or(false) {
                    return false;
                }
                mw.collision.collision_shape(&mut scene, state, p).ok().is_some_and(|shape| {
                    let Ok(eye_shape) = VoxelShape::from_box(*eye_bounds) else {
                        return false;
                    };
                    shape
                        .moved(Vec3::new(f64::from(x), f64::from(y), f64::from(z)))
                        .join_is_not_empty(&eye_shape, ShapeOp::AND)
                        .unwrap_or(false)
                })
            },
        );
        let mut cmd = commands.entity(e);
        cmd.remove::<Recheck>();
        match (inside, was) {
            (true, false) => {
                cmd.insert(InWall);
            }
            (false, true) => {
                cmd.remove::<InWall>();
            }
            _ => {}
        }
    }
}
fn wall_damage(q: Query<Entity, With<InWall>>, mut queue: ResMut<DamageQueue>) {
    for e in &q {
        queue.0.push((e, 1.0, Cause::InWall));
    }
}

/// The index follows the boxes that changed (the cows that moved): only a change of column touches it; a change of
/// the hazards under the box adds or takes off the `Hazard` module.
#[allow(clippy::type_complexity)]
fn reindex(
    mut idx: ResMut<Index>,
    hazards: Res<Hazards>,
    mut q: Query<(Entity, &Bounds, &mut Cell, Option<&Hazard>, Has<InFluid>, &mut FluidState), Changed<Bounds>>,
    mut commands: Commands,
) {
    for (e, b, mut c, hz, in_fluid, mut fs) in &mut q {
        let (mn, mx) = (b.0.min_position(), b.0.max_position());
        let cell = idx.cell_of((mn.x + mx.x) / 2.0, (mn.z + mx.z) / 2.0);
        if cell != c.0 {
            if c.0 != u32::MAX {
                idx.remove(e, c.0);
            }
            idx.insert(e, cell);
            c.0 = cell;
        }
        let bits = hazards.of_box(&b.0);
        let fluid = bits & (HAZ_WATER | HAZ_LAVA) != 0;
        if fluid && !in_fluid {
            commands.entity(e).insert(InFluid);
        } else if !fluid && in_fluid {
            commands.entity(e).remove::<InFluid>();
            *fs = FluidState::default();
        }
        match (bits, hz) {
            (0, Some(_)) => {
                commands.entity(e).remove::<Hazard>();
            }
            (0, None) => {}
            (bits, Some(h)) if h.0 == bits => {}
            (bits, _) => {
                commands.entity(e).insert(Hazard(bits));
            }
        }
    }
}

/// `LivingEntity.hurt`: the queued damage is applied; within 10 ticks of a hurt only a larger hit counts, by the
/// difference; a cow at 0 health dies (leaves the index and the world).
fn apply_damage(
    clock: Res<Clock>,
    mut queue: ResMut<DamageQueue>,
    mut idx: ResMut<Index>,
    mut knock: ResMut<Knockbacks>,
    mut q: Query<(&mut Health, &mut HurtAt, &Cell)>,
    mut bodies: Query<(&mut Vel, &Contact)>,
    mut hurts: ResMut<crate::goals::Hurts>,
    mut causes: ResMut<HurtCauses>,
    mut commands: Commands,
) {
    for (e, dx, dz) in std::mem::take(&mut knock.0) {
        // `LivingEntity.knockback(0.4, dx, dz)`: away from the attacker, a hop when on the ground.
        let Ok((mut v, c)) = bodies.get_mut(e) else { continue };
        let l = (dx * dx + dz * dz).sqrt();
        if l >= 1.0e-5 {
            let (nx, nz) = (dx / l * 0.4, dz / l * 0.4);
            let y = if c.on_ground { (v.0.y / 2.0 + 0.4).min(0.4) } else { v.0.y };
            v.0 = Vec3::new(v.0.x / 2.0 - nx, y, v.0.z / 2.0 - nz);
            commands.entity(e).insert(Moving);
        }
    }
    // The causes of the previous step were read by the goals (they run after this system in the chain).
    causes.0.clear();
    for (e, amount, cause) in std::mem::take(&mut queue.0) {
        let Ok((mut h, mut hurt, cell)) = q.get_mut(e) else { continue };
        let applied = if clock.0 < hurt.at + 10 && hurt.at != 0 {
            if amount <= hurt.last {
                continue;
            }
            amount - hurt.last
        } else {
            amount
        };
        *hurt = HurtAt { at: clock.0, last: amount };
        h.0 -= applied;
        if h.0 <= 0.0 {
            if cell.0 != u32::MAX {
                idx.remove(e, cell.0);
            }
            commands.entity(e).despawn();
        } else {
            hurts.0.push(e);
            causes.0.push((e, cause));
        }
    }
}
/// The module of the cows that overlap a hazard (only they are visited): lava ignites 15 s and hurts 4, fire ignites
/// 8 s, a cactus hurts 1, powder snow counts the freezing (1 damage every 40 ticks once frozen for 140), water puts
/// the fire out. The fire's own damage (1 every 20 ticks) is in the timer wheel.
#[allow(clippy::type_complexity)]
fn hazard_tick(
    clock: Res<Clock>,
    mut wheel: ResMut<Wheel>,
    mut queue: ResMut<DamageQueue>,
    mut q: Query<(Entity, &Hazard, &mut Frozen, Option<&Burning>)>,
    mut commands: Commands,
) {
    let now = clock.0;
    for (e, h, mut frozen, burning) in &mut q {
        let f = h.0;
        if f & HAZ_WATER != 0 {
            if burning.is_some() {
                commands.entity(e).remove::<Burning>();
            }
        } else {
            let seconds = if f & HAZ_LAVA != 0 { 300 } else if f & HAZ_FIRE != 0 { 160 } else { 0 };
            if seconds > 0 {
                let until = now + seconds;
                match burning {
                    None => {
                        wheel.push(now + 20, e, Due::Burn);
                        commands.entity(e).insert(Burning { until });
                    }
                    Some(b) if b.until < until => {
                        commands.entity(e).insert(Burning { until });
                    }
                    _ => {}
                }
            }
        }
        if f & HAZ_LAVA != 0 {
            queue.0.push((e, 4.0, Cause::Lava));
        }
        if f & HAZ_FIRE != 0 {
            // `FireBlock.entityInside`: the fire hurts 1 as well as setting alight.
            queue.0.push((e, 1.0, Cause::InFire));
        }
        if f & HAZ_CACTUS != 0 {
            queue.0.push((e, 1.0, Cause::Cactus));
        }
        if f & HAZ_SNOW != 0 {
            frozen.0 = (frozen.0 + 1).min(140);
            if frozen.0 >= 140 && now % 40 == 0 {
                queue.0.push((e, 1.0, Cause::Freeze));
            }
        }
    }
}
/// A velocity a cow that did not move is owed by a push (applied by `apply_pushes`).
#[derive(Component, Clone, Copy, Debug)]
pub struct PushIn(pub Vec3);
/// `LivingEntity.pushEntities` / `Entity.push` for the boxes that moved this step (the only ones that can start
/// overlapping something), in parallel: each moved cow reads the boxes of the neighbours it overlaps (the index gives
/// the candidates, the boxes are components, final for this step) and changes ONLY ITS OWN velocity (its half); the
/// neighbour that did not move gets its half as a command (a `PushIn`), which also wakes its physics.
#[allow(clippy::type_complexity)]
fn push(
    cfg: Res<Config>,
    idx: Res<Index>,
    par: ParallelCommands,
    boxes: Query<Ref<Bounds>>,
    mut movers: Query<(Entity, &Bounds, &mut Vel, Has<Moving>), Changed<Bounds>>,
) {
    let one = |e: Entity, b: &Bounds, v: &mut Vel, moving: bool| {
        let (mn, mx) = (b.0.min_position(), b.0.max_position());
        let (cx, cz) = ((mn.x + mx.x) / 2.0, (mn.z + mx.z) / 2.0);
        let mut dv = (0.0_f64, 0.0_f64);
        idx.for_near(cx, cz, |o| {
            if o == e {
                return;
            }
            let Ok(ob) = boxes.get(o) else { return };
            if !b.0.intersects(ob.0) {
                return;
            }
            let (omn, omx) = (ob.0.min_position(), ob.0.max_position());
            let (mut xa, mut za) = ((omn.x + omx.x) / 2.0 - cx, (omn.z + omx.z) / 2.0 - cz);
            let mut dd = xa.abs().max(za.abs());
            if dd < f64::from(0.01_f32) {
                return;
            }
            dd = dd.sqrt();
            xa /= dd;
            za /= dd;
            let pow = (1.0_f64 / dd).min(1.0);
            xa *= pow * f64::from(0.05_f32);
            za *= pow * f64::from(0.05_f32);
            dv.0 -= xa;
            dv.1 -= za;
            if !ob.is_changed() {
                par.command_scope(|mut cmd| {
                    cmd.entity(o).insert((PushIn(Vec3::new(xa, 0.0, za)), Moving));
                });
            }
        });
        if dv != (0.0, 0.0) {
            v.0 = Vec3::new(v.0.x + dv.0, v.0.y, v.0.z + dv.1);
            if !moving {
                par.command_scope(|mut cmd| {
                    cmd.entity(e).insert(Moving);
                });
            }
        }
    };
    if cfg.parallel {
        movers.par_iter_mut().for_each(|(e, b, mut v, m)| one(e, b, &mut v, m));
    } else {
        for (e, b, mut v, m) in &mut movers {
            one(e, b, &mut v, m);
        }
    }
}
/// The half of a push a cow that did not move is owed: its velocity changes, its physics is active.
fn apply_pushes(mut q: Query<(Entity, &PushIn, &mut Vel)>, mut commands: Commands) {
    for (e, p, mut v) in &mut q {
        v.0 = Vec3::new(v.0.x + p.0.x, v.0.y, v.0.z + p.0.z);
        commands.entity(e).remove::<PushIn>();
    }
}

/// What is due at this tick in the timer wheel: a stroll attempt (a spot is picked, or the attempt fails and is
/// armed again, or the goal is refused for lack of a player and is not re-armed), a baby grows up.
#[allow(clippy::type_complexity)]
pub(crate) fn due(
    mut clock: ResMut<Clock>,
    mut wheel: ResMut<Wheel>,
    mut fired: ResMut<crate::goals::Fired>,
    mut babies: Query<(&mut Body, &Sizes, &GrowAt, &Pos, &mut Bounds)>,
    burners: Query<(Entity, &Burning)>,
    mut queue: ResMut<DamageQueue>,
    mut commands: Commands,
) {
    clock.0 += 1;
    let slot = (clock.0 % WHEEL as u64) as usize;
    let list = std::mem::take(&mut wheel.slots[slot]);
    for (e, d) in list {
        match d {
            // The goal selector (`goals.rs`) owns the stroll attempt, like every other goal's deadline.
            Due::Stroll => fired.0.push((e, crate::goals::Kind::Stroll)),
            Due::Goal(k) => fired.0.push((e, k)),
            Due::Burn => {
                if let Ok((e2, burning)) = burners.get(e) {
                    if clock.0 < burning.until {
                        queue.0.push((e2, 1.0, Cause::OnFire));
                        wheel.push((clock.0 + 20).min(clock.0 + WHEEL as u64 - 1), e, Due::Burn);
                    } else {
                        commands.entity(e).remove::<Burning>();
                    }
                }
            }
            Due::Grow => {
                if let Ok((mut b, sizes, g, p, mut bb)) = babies.get_mut(e) {
                    if g.0 > clock.0 {
                        // A wait longer than the wheel is put back (a calf grows in 24 000 ticks).
                        wheel.push(g.0.min(clock.0 + WHEEL as u64 - 1), e, Due::Grow);
                    } else if g.0 == clock.0 {
                        b.width = sizes.adult.0;
                        b.height = sizes.adult.1;
                        bb.0 = box_at(p.0, sizes.adult.0, sizes.adult.1);
                        commands.entity(e).remove::<GrowAt>().insert(Recheck);
                    }
                }
            }
        }
    }
}

pub struct Herd {
    pub world: World,
    pub schedule: Schedule,
}
impl Herd {
    /// `n` cows spaced `spacing` blocks apart on a slab just large enough.
    pub fn new(n: usize, spacing: f64, sleep: bool, parallel: bool, seed: u64) -> Self {
        Self::with_hazards(n, spacing, sleep, parallel, seed, false)
    }
    /// With `hazards`, 3 x 3 patches of lava, water, powder snow, fire and cactus every 48 blocks.
    pub fn with_hazards(n: usize, spacing: f64, sleep: bool, parallel: bool, seed: u64, hazards: bool) -> Self {
        let side = (n as f64).sqrt().ceil() as i32;
        let half = ((f64::from(side) * spacing / 2.0) as i32 + 6).max(20);
        let mut world = World::new();
        let (level, species, _kind) = build_level(half, hazards);
        world.insert_resource(Hazards::build(&level));
        world.insert_resource(DamageQueue::default());
        world.insert_resource(Knockbacks::default());
        world.insert_resource(HurtCauses::default());
        world.insert_resource(level);
        world.insert_resource(species);
        world.insert_resource(Config { sleep, parallel });
        world.insert_resource(Clock::default());
        world.insert_resource(Players::default());
        world.insert_resource(Wheel::default());
        crate::goals::insert_resources(&mut world);
        world.insert_resource(Index::new(-f64::from(half), (2 * half / 2) as usize + 2));
        let mut rng = Rng(seed | 1);
        for i in 0..n {
            let (cx, cz) = ((i as i32 % side) as f64, (i as i32 / side) as f64);
            let o = -f64::from(side) * spacing / 2.0;
            let s = u64::from(rng.next_u32()) << 32 | u64::from(rng.next_u32());
            world.spawn(cow_bundle(o + cx * spacing + 0.5, o + cz * spacing + 0.5, s));
        }
        {
            let mut arm = Rng(seed ^ 0x9e37_79b9_7f4a_7c15 | 1);
            let ids: Vec<Entity> = world.query_filtered::<Entity, With<Stroll>>().iter(&world).collect();
            for e in ids {
                let at = 1 + idle_wait(&mut arm, 60).max(0) as u64;
                world.resource_mut::<Wheel>().push(at.min(WHEEL as u64 - 1), e, Due::Stroll);
                world.get_mut::<Stroll>(e).unwrap().scheduled = true;
            }
        }
        crate::goals::arm_all(&mut world);
        // The cows are listed in the index from the start (a player event is read at the first tick).
        world.run_system_cached(reindex).unwrap();
        Self { world, schedule: schedule() }
    }
    /// The players of the herd's world (their positions; cows within 32 blocks stay active).
    pub fn set_players(&mut self, players: Vec<(f64, f64)>) {
        let mut p = self.world.resource_mut::<Players>();
        p.list = players;
        p.dirty = true;
    }
    /// A hit from `from` (x, z): `amount` damage and the knockback away from it (the damage a player does).
    pub fn hit(&mut self, e: Entity, amount: f32, from: (f64, f64)) {
        let p = self.world.get::<Pos>(e).unwrap().0;
        self.world.resource_mut::<DamageQueue>().0.push((e, amount, Cause::Entity));
        // `knockback(0.4, attacker - victim)`: the direction TOWARD the attacker, the push is the other way.
        self.world.resource_mut::<Knockbacks>().0.push((e, from.0 - p.x, from.1 - p.z));
    }
    /// Puts the cow at `at`: its box follows, and the events that can put it into blocks or a fluid are raised.
    pub fn teleport(&mut self, e: Entity, at: Vec3) {
        let b = *self.world.get::<Body>(e).unwrap();
        self.world.get_mut::<Pos>(e).unwrap().0 = at;
        self.world.get_mut::<Vel>(e).unwrap().0 = Vec3::ZERO;
        self.world.get_mut::<Bounds>(e).unwrap().0 = box_at(at, b.width, b.height);
        self.world.entity_mut(e).insert((Recheck, Moving));
    }
    pub fn tick(&mut self) {
        self.schedule.run(&mut self.world);
    }
    /// The cow grows up in `ticks` ticks (it is a baby until then: the baby's size, one wheel entry).
    pub fn make_baby(&mut self, e: Entity, ticks: u64) {
        let now = self.world.resource::<Clock>().0;
        let sizes = *self.world.get::<Sizes>(e).unwrap();
        let p = self.world.get::<Pos>(e).unwrap().0;
        let mut b = self.world.get_mut::<Body>(e).unwrap();
        b.width = sizes.baby.0;
        b.height = sizes.baby.1;
        self.world.get_mut::<Bounds>(e).unwrap().0 = box_at(p, sizes.baby.0, sizes.baby.1);
        self.world.entity_mut(e).insert(GrowAt(now + ticks));
        self.world.resource_mut::<Wheel>().push(now + ticks, e, Due::Grow);
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
    }
    pub const SYSTEMS: [&'static str; 12] = ["due", "player_near", "pathfind", "follow", "move_control", "travel", "reindex", "push", "path_ended", "apply_pushes", "hazard_tick", "apply_damage"];
    /// (cows with no active module, cows that follow a path, cows whose physics is active, total)
    pub fn counts(&mut self) -> (usize, usize, usize, usize) {
        let following = self.world.query_filtered::<(), With<Following>>().iter(&self.world).count();
        let moving = self.world.query_filtered::<(), With<Moving>>().iter(&self.world).count();
        let idle = self.world.query_filtered::<(), (Without<Moving>, Without<Following>)>().iter(&self.world).count();
        let total = self.world.query::<&Pos>().iter(&self.world).count();
        (idle, following, moving, total)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two cows standing in each other are pushed apart; a sleeping cow they overlap is woken; a baby grows up.
    #[test]
    fn cows_push_each_other_and_babies_grow() {
        if std::env::var_os("HORIZON_TEST_RESOURCES").is_none() {
            return;
        }
        let mut herd = Herd::new(0, 6.0, true, false, 3);
                let a = herd.world.spawn(cow_bundle(0.5, 0.5, 11)).id();
        let b = herd.world.spawn(cow_bundle(0.8, 0.5, 12)).id();
        let baby = herd.world.spawn(cow_bundle(10.5, 10.5, 13)).id();
        herd.make_baby(baby, 30);
        for _ in 0..4 {
            herd.tick();
        }
        let gap = |herd: &Herd| {
            let (pa, pb) = (herd.world.get::<Pos>(a).unwrap().0, herd.world.get::<Pos>(b).unwrap().0);
            ((pa.x - pb.x).powi(2) + (pa.z - pb.z).powi(2)).sqrt()
        };
        let start = 0.3;
        for _ in 0..20 {
            herd.tick();
        }
        assert!(gap(&herd) > start, "the cows were not pushed apart: {}", gap(&herd));
        assert!(herd.world.get::<Body>(baby).unwrap().width < 0.5);
        for _ in 0..40 {
            herd.tick();
        }
        assert!(herd.world.get::<Body>(baby).unwrap().width > 0.8);
    }

    /// Palier 2 on single cows (no cross reading): a fall, lava, fire, powder snow, a cactus, by events.
    #[test]
    fn falls_fire_lava_snow_and_cactus_hurt_a_cow() {
        if std::env::var_os("HORIZON_TEST_RESOURCES").is_none() {
            return;
        }
        // half = 56: the patches are at (-48, -48) and (0, 0), 3 x 3 each (lava, pond, snow in the floor; fire, cactus on it).
        let mut herd = Herd::with_hazards(1, 100.0, true, false, 5, true);
        let mut cow = |herd: &mut Herd, x: f64, y: f64, z: f64| {
            let e = herd.world.spawn(cow_bundle(x, z, 77)).id();
            herd.world.get_mut::<Pos>(e).unwrap().0.y = y;
            herd.world.get_mut::<Bounds>(e).unwrap().0 = box_at(Vec3::new(x, y, z), 0.9, 1.4);
            herd.world.entity_mut(e).insert(ActiveUntil(0));
            e
        };
        let health = |herd: &Herd, e: Entity| herd.world.get::<Health>(e).map(|h| h.0);
        let (hurt, dies) = (cow(&mut herd, 5.5, 12.0, 5.5), cow(&mut herd, 8.5, 40.0, 5.5));
        let lava = cow(&mut herd, -47.0, 1.0, -47.0);
        let fire = cow(&mut herd, -47.0, 1.0, -39.0);
        let snow = cow(&mut herd, -31.0, 1.0, -47.0);
        let cactus = cow(&mut herd, -39.0, 1.0, -39.0);
        let pond = cow(&mut herd, -39.0, 3.0, -47.0);
        // No float goal for this cow (the Mob case has none either): it sinks and drowns.
        herd.world.entity_mut(pond).remove::<crate::goals::Jump>();
        let wall = cow(&mut herd, 20.5, 1.0, 20.5);
        herd.teleport(wall, Vec3::new(20.5, -0.6, 20.5));
        let show = |herd: &Herd, t: u32| {
            let h = |e: Entity| health(herd, e).map_or("dead".to_string(), |h| format!("{h}"));
            eprintln!("tick {t}: fall 11: {}, fall 39: {}, lava: {}, fire: {}, powder snow: {}, cactus: {}, pond: {}, in wall: {}", h(hurt), h(dies), h(lava), h(fire), h(snow), h(cactus), h(pond), h(wall));
        };
        for _ in 0..40 {
            herd.tick();
        }
        show(&herd, 40);
        // The fall: 11 blocks hurt by 8 (ceil(11 - 3)); 39 kill.
        assert!(health(&herd, hurt).is_some_and(|h| h > 0.0 && h < 10.0), "{:?}", health(&herd, hurt));
        assert!(health(&herd, dies).is_none(), "the cow that fell 39 blocks lives");
        assert!(health(&herd, cactus).is_some_and(|h| h < 10.0), "the cactus did not hurt");
        for _ in 0..100 {
            herd.tick();
        }
        show(&herd, 140);
        assert!(health(&herd, lava).is_none(), "the cow in lava lives: {:?}", health(&herd, lava));
        assert!(health(&herd, fire).is_none_or(|h| h <= 6.0), "the fire: {:?}", health(&herd, fire));
        for _ in 0..200 {
            herd.tick();
        }
        show(&herd, 340);
        assert!(health(&herd, snow).is_some_and(|h| h < 10.0), "the powder snow: {:?}", health(&herd, snow));
        assert!(herd.world.get::<Burning>(fire).is_some() || health(&herd, fire).is_none());
        assert!(health(&herd, pond).is_none_or(|h| h < 10.0), "the cow in the pond did not drown");
        assert!(health(&herd, wall).is_none(), "the cow in the wall lives");
    }

    /// The cactus cow of the hazard test, tick by tick (position, health, whether a walk or a path started).
    #[test]
    #[ignore = "trace"]
    fn trace_the_cactus_cow() {
        let mut herd = Herd::with_hazards(1, 100.0, true, false, 5, true);
        let e = herd.world.spawn(cow_bundle(-39.0, -39.0, 77)).id();
        herd.world.get_mut::<Pos>(e).unwrap().0.y = 1.0;
        herd.world.entity_mut(e).insert(ActiveUntil(0));
        for t in 1..=40 {
            herd.tick();
            let p = herd.world.get::<Pos>(e).map(|p| p.0);
            let h = herd.world.get::<Health>(e).map(|h| h.0);
            let walking = herd.world.get::<Stroll>(e).map(|s| s.walking);
            let hz = herd.world.get::<Hazard>(e).map(|h| h.0);
            eprintln!("ecs cactus t={t:3} pos={p:?} health={h:?} walking={walking:?} hazard={hz:?}");
        }
    }

    /// A cow driven east into the cactus patch (the gesture that counts in play), tick by tick, to compare with `Mob`.
    #[test]
    #[ignore = "trace"]
    fn trace_a_cow_walking_into_a_cactus() {
        let mut herd = Herd::with_hazards(1, 100.0, true, false, 5, true);
        let e = herd.world.spawn(cow_bundle(-42.5, -39.0, 77)).id();
        herd.world.entity_mut(e).insert(ActiveUntil(0));
        herd.world.get_mut::<Yaw>(e).unwrap().0 = -90.0;
        herd.world.get_mut::<Drive>(e).unwrap().speed = 0.2;
        herd.world.get_mut::<Drive>(e).unwrap().zza = 0.2;
        for t in 1..=90 {
            herd.tick();
            if t % 5 == 0 {
                let p = herd.world.get::<Pos>(e).map(|p| p.0);
                let h = herd.world.get::<Health>(e).map(|h| h.0);
                eprintln!("ecs walk t={t:3} pos={p:?} health={h:?}");
            }
        }
    }

    /// A hit from the east pushes the cow west with a hop (knockback 0.4), and wakes its physics.
    #[test]
    fn a_hit_knocks_a_cow_back() {
        if std::env::var_os("HORIZON_TEST_RESOURCES").is_none() {
            return;
        }
        let mut herd = Herd::new(1, 100.0, true, false, 5);
        let e = herd.world.spawn(cow_bundle(10.5, 10.5, 9)).id();
        herd.world.entity_mut(e).insert(ActiveUntil(0));
        for _ in 0..5 {
            herd.tick();
        }
        assert!(herd.world.get::<Moving>(e).is_none(), "the cow was not at rest");
        herd.hit(e, 1.0, (11.5, 10.5));
        herd.tick();
        // The push is applied at the end of the step (an event); the cow moves in the next one.
        herd.tick();
        let v = herd.world.get::<Vel>(e).unwrap().0;
        assert!(herd.world.get::<Moving>(e).is_some());
        assert!(herd.world.get::<Pos>(e).unwrap().0.x < 10.5 && herd.world.get::<Pos>(e).unwrap().0.y > 1.0, "{v:?}");
    }

    /// Common sense: the cows stay on the slab and on its ground, walk, fall asleep and wake.
    #[test]
    fn the_cows_walk_sleep_and_wake_on_the_slab() {
        if std::env::var_os("HORIZON_TEST_RESOURCES").is_none() {
            return;
        }
        let mut herd = Herd::new(300, 3.0, true, false, 3);
        herd.set_players(vec![(0.0, 0.0)]);
        let half = f64::from(herd.world.resource::<Level>().half);
        let start: Vec<Vec3> = herd.world.query::<&Pos>().iter(&herd.world).map(|p| p.0).collect();
        let (mut max_asleep, mut min_asleep) = (0, usize::MAX);
        for t in 0..800 {
            herd.tick();
            if t >= 400 {
                let (asleep, _, _, _) = herd.counts();
                max_asleep = max_asleep.max(asleep);
                min_asleep = min_asleep.min(asleep);
            }
        }
        let now: Vec<Vec3> = herd.world.query::<&Pos>().iter(&herd.world).map(|p| p.0).collect();
        let travelled = start.iter().zip(&now).filter(|(a, b)| (a.x - b.x).abs() + (a.z - b.z).abs() > 3.0).count();
        for p in &now {
            assert!(p.x.abs() < half && p.z.abs() < half && p.y > 0.99 && p.y < 1.2, "{p:?}");
        }
        assert!(travelled > 100, "only {travelled} cows travelled more than 3 blocks");
        assert!(max_asleep > 30 && min_asleep < 300, "{min_asleep}..{max_asleep} asleep of 300");
        // The player leaves: after 100 ticks every cow is idle and asleep, for good.
        herd.set_players(vec![]);
        for _ in 0..400 {
            herd.tick();
        }
        assert!(herd.counts().0 >= 295, "{} asleep after the player left", herd.counts().0);
        // And the player comes back: the cows around it wake.
        herd.set_players(vec![(0.0, 0.0)]);
        let mut least = 300;
        for _ in 0..300 {
            herd.tick();
            least = least.min(herd.counts().0);
        }
        assert!(least < 250, "{least} idle at the least with a player there");
        eprintln!("travelled {travelled}, asleep between {min_asleep} and {max_asleep} of 300");
    }
}
