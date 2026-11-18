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
    /// The move control asks for a jump (`MoveControl`: the target is above the step height and near).
    pub jump: bool,
    /// The jump input (a goal sets it: `FloatGoal` keeps it while the cow is in deep water).
    pub jumping: bool,
    /// `Mob.noJumpDelay`: ticks before the cow may jump again (10 after a jump, 0 when the input stops).
    pub no_jump_delay: i32,
    /// `MoveControl.Operation.JUMPING`: after a jump the move control only keeps the speed until the cow lands.
    pub jumping_op: bool,
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
    /// The block row of the path target (1 on the slab; a water spot is one lower).
    pub ty: i32,
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
    /// `PathNavigation.timeLastRecompute` and `delayedRecompute`.
    pub last_recompute: u64,
    pub delayed: bool,
}
impl Default for Nav {
    fn default() -> Self {
        Self { path: None, last_check: 0, last_pos: Vec3::ZERO, speed: 1.0, last_recompute: 0, delayed: false }
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
pub struct DamageQueue(pub Vec<Hit>);
/// One hit: who, how much, what kind, and from where when an entity did it (the knockback comes from it).
#[derive(Clone, Copy, Debug)]
pub struct Hit {
    pub target: Entity,
    pub amount: f32,
    pub cause: Cause,
    /// The attacker's position (x, z); `None` for an attacker without one or an environmental cause.
    pub from: Option<(f64, f64)>,
    /// The player that did it (an entity of the bench's `Online` list), for the target of a `HurtByTarget`.
    pub by: Option<u32>,
}
impl DamageQueue {
    pub fn add(&mut self, target: Entity, amount: f32, cause: Cause) {
        self.0.push(Hit { target, amount, cause, from: None, by: None });
    }
    pub fn add_by(&mut self, target: Entity, amount: f32, from: (f64, f64), player: u32) {
        self.0.push(Hit { target, amount, cause: Cause::Entity, from: Some(from), by: Some(player) });
    }
    pub fn add_from(&mut self, target: Entity, amount: f32, cause: Cause, from: (f64, f64)) {
        self.0.push(Hit { target, amount, cause, from: Some(from), by: None });
    }
}
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
/// The block changes waiting for the next step: (x, y, z, block name).
#[derive(Resource, Default)]
pub struct BlockChanges(pub Vec<(i32, i32, i32, &'static str)>);
/// The hazard blocks of the world by column and height: only the columns that hold one have an entry. A cow's hazard
/// bits are those of the entries its box (widened by a block each way, as a prefilter) meets; a block change at any
/// height updates its entry (`set`).
#[derive(Resource, Default)]
pub struct Hazards {
    origin: i32,
    width: usize,
    /// Per column: 0, or 1 + the index in `pool`.
    cols: Vec<u32>,
    pool: Vec<Vec<(i32, u8)>>,
}
fn hazard_bit(name: &str) -> u8 {
    match name {
        "minecraft:lava" => HAZ_LAVA,
        "minecraft:fire" => HAZ_FIRE,
        "minecraft:water" => HAZ_WATER,
        "minecraft:powder_snow" => HAZ_SNOW,
        "minecraft:cactus" => HAZ_CACTUS,
        _ => 0,
    }
}
impl Hazards {
    /// Reads the generated height of the world (`-2..=BUILD_TOP`: the floor, the walls and what the patches add).
    pub fn build(level: &Level) -> Self {
        let mw = level.mw.0;
        let (origin, width) = (-level.half, (2 * level.half + 1) as usize);
        let mut h = Self { origin, width, cols: vec![0; width * width], pool: Vec::new() };
        for x in 0..width {
            for z in 0..width {
                for y in -2..=BUILD_TOP {
                    let bit = hazard_bit(&mw.facts.get(mw.world.block(x as i32 + origin, y, z as i32 + origin)).name);
                    if bit != 0 {
                        h.set(x as i32 + origin, y, z as i32 + origin, bit);
                    }
                }
            }
        }
        h
    }
    /// The block at (x, y, z) now has these hazard bits (0: none), at any height.
    pub fn set(&mut self, x: i32, y: i32, z: i32, bit: u8) {
        let (cx, cz) = (x - self.origin, z - self.origin);
        if cx < 0 || cz < 0 || cx as usize >= self.width || cz as usize >= self.width {
            return;
        }
        let i = cz as usize * self.width + cx as usize;
        if self.cols[i] == 0 {
            if bit == 0 {
                return;
            }
            self.pool.push(Vec::new());
            self.cols[i] = self.pool.len() as u32;
        }
        let list = &mut self.pool[self.cols[i] as usize - 1];
        list.retain(|e| e.0 != y);
        if bit != 0 {
            list.push((y, bit));
        }
    }
    /// The hazards of the columns a box covers, at the heights it covers (a block more each way).
    pub fn of_box(&self, b: &Aabb) -> u8 {
        let (mn, mx) = (b.min_position(), b.max_position());
        let e = 1.0e-7;
        // Fluids have a surface below the top of their block and flow: they are looked at a block wider (the real
        // test is `fluids`); fire, cactus and snow are the blocks the box overlaps.
        let (y0, y1) = (mn.y.floor() as i32 - 1, (mx.y - e).floor() as i32 + 1);
        let (x0, x1) = ((mn.y + 0.001).floor() as i32, (mx.y - 0.001).floor() as i32);
        let mut bits = 0;
        for x in mn.x.floor() as i32..=(mx.x - e).floor() as i32 {
            for z in mn.z.floor() as i32..=(mx.z - e).floor() as i32 {
                let (cx, cz) = (x - self.origin, z - self.origin);
                if cx < 0 || cz < 0 || cx as usize >= self.width || cz as usize >= self.width {
                    continue;
                }
                let c = self.cols[cz as usize * self.width + cx as usize];
                if c != 0 {
                    for &(y, bit) in &self.pool[c as usize - 1] {
                        let fluid = bit & (HAZ_WATER | HAZ_LAVA) != 0;
                        if (fluid && (y0..=y1).contains(&y)) || (!fluid && (x0..=x1).contains(&y)) {
                            bits |= bit;
                        }
                    }
                }
            }
        }
        bits
    }
}
/// The top of what the bench worlds generate (the patches reach y = 3).
const BUILD_TOP: i32 = 6;
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
    /// `Mob.isSunBurnTick`'s draw came up (`hostile.rs`).
    Sun,
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
    /// How many entries the slot of tick `at` holds.
    pub fn due_len(&self, at: u64) -> usize {
        self.slots[(at % WHEEL as u64) as usize].len()
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
/// The leaked `World` of the blocks, for the one system that changes them (`block_changes`, which runs alone in the
/// chain: no other system holds a reference then).
#[derive(Clone, Copy)]
pub struct Blocks(pub *mut horizon_client_render::world::World);
unsafe impl Send for Blocks {}
unsafe impl Sync for Blocks {}
#[derive(Resource, Clone, Copy)]
pub struct Level {
    pub mw: Mw,
    pub blocks: Blocks,
    /// The walls are at |x| or |z| = `half`.
    pub half: i32,
}
#[derive(Resource, Clone, Copy)]
pub struct Config {
    pub sleep: bool,
    pub parallel: bool,
}
/// The species' navigation data, read once from a real `Mob` of the type.
/// The species of an entity: an index into the `SpeciesTable`.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpeciesId(pub u8);
/// The data of every species of the world, by `SpeciesId`: what a system needs of a species it reads here, per entity.
#[derive(Resource, Clone)]
pub struct SpeciesTable(pub Vec<Species>);
#[derive(Clone, Copy)]
pub struct Species {
    pub id: u8,
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
    /// The type (`minecraft:cow`...), whose size, speed and goals below are data of the species.
    pub name: &'static str,
    pub width: f32,
    pub height: f32,
    pub step_height: f32,
    pub gravity: f64,
    pub speed: f64,
    pub baby: (f32, f32),
    pub goals: crate::goals::GoalDef,
    /// The farm behaviours of the species (`farm.rs`).
    pub farm: crate::farm::FarmDef,
    /// `Attributes.KNOCKBACK_RESISTANCE`.
    pub knockback_resistance: f64,
    /// `Attributes.ATTACK_DAMAGE` (0 for a species that does not attack).
    pub attack_damage: f32,
    /// `EntityType` flag: the undead that burn in daylight.
    pub burns_in_day: bool,
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
            // A low ceiling (stone at y = 3, 3 x 3) for the jump delay: a cow under it jumps for less than 10 ticks.
            put(blocks, "minecraft:stone", -20, -20, 3);
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
    let (mw, types, blocks_ptr) =
        horizon_client_lib::mobs::world::leaked_test_world_mut(true, build).expect("HORIZON_TEST_RESOURCES");
    let chosen: &'static str = Box::leak(
        std::env::var("HORIZON_SPECIES").unwrap_or_else(|_| "minecraft:cow".into()).into_boxed_str(),
    );
    // Every species the prototype knows, by id: the same table for the whole process.
    let names: [&'static str; 4] = ["minecraft:cow", "minecraft:pig", "minecraft:zombie", "minecraft:sheep"];
    let mut table: Vec<Species> = Vec::new();
    let mut chosen_kind = None;
    for (i, name) in names.into_iter().enumerate() {
        let kind = Arc::new(types.types[name].clone());
        let mob = Mob::new(1, kind.clone(), Vec3::new(0.5, 1.0, 0.5), 0.0);
        table.push(Species {
            id: i as u8,
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
            name,
            width: kind.width as f32,
            height: kind.height as f32,
            step_height: kind.step_height as f32,
            gravity: kind.gravity,
            speed: mob.attrs.speed,
            baby: ((kind.width * kind.baby_scale) as f32, (kind.height * kind.baby_scale) as f32),
            goals: crate::goals::goal_def(name),
            farm: crate::farm::farm_def(name),
            knockback_resistance: mob.attrs.knockback_resistance,
            attack_damage: mob.attrs.attack_damage,
            burns_in_day: kind.flags.burns_in_day,
        });
        if name == chosen {
            chosen_kind = Some(kind);
        }
    }
    let _ = TYPES.set(types);
    let _ = TABLE.set(table.clone());
    let species = *table.iter().find(|s| s.name == chosen).expect("a known species");
    let _ = SPECIES.set(species);
    (Level { mw: Mw(mw), blocks: Blocks(blocks_ptr), half }, species, chosen_kind.expect("a known species"))
}
/// The prepared vanilla types of the process, for the species the tests compare with a `Mob`.
static TYPES: std::sync::OnceLock<&'static horizon_client_lib::mobs::MobTypes> = std::sync::OnceLock::new();
pub fn mob_kind(name: &str) -> Arc<horizon_client_lib::mobs::MobType> {
    Arc::new(TYPES.get().expect("a level was built").types[name].clone())
}
/// Every species of the process (see `build_level`).
static TABLE: std::sync::OnceLock<Vec<Species>> = std::sync::OnceLock::new();
pub fn species_table() -> SpeciesTable {
    SpeciesTable(TABLE.get().expect("a level was built").clone())
}

/// For the tests of a behaviour only some species have: whether to skip the test of the process's species, which has no
/// such goal (the skip is printed, so it is never silent).
pub fn skip_unless(kind: crate::goals::Kind, test: &str) -> bool {
    // By the name the process was asked for (`HORIZON_SPECIES`), before any level is built.
    let name = std::env::var("HORIZON_SPECIES").unwrap_or_else(|_| "minecraft:cow".into());
    let has = crate::goals::goal_def(&name).slots.iter().any(|s| s.0 == kind);
    if !has {
        eprintln!("SKIP {test}: {name} has no {kind:?} goal");
    }
    !has
}
/// The reverse: a test of a passive behaviour (a player in range changes nothing) skips a species that has the goal
/// (a zombie hunts the player the test puts in range), printed.
pub fn skip_if(kind: crate::goals::Kind, test: &str) -> bool {
    let name = std::env::var("HORIZON_SPECIES").unwrap_or_else(|_| "minecraft:cow".into());
    let has = crate::goals::goal_def(&name).slots.iter().any(|s| s.0 == kind);
    if has {
        eprintln!("SKIP {test}: {name} has a {kind:?} goal: the player in range makes it hunt, the test measures passive looking");
    }
    has
}
/// The species of the process (the first world built sets it: `HORIZON_SPECIES`, `minecraft:cow` by default), for
/// the bundles that are made without a world at hand.
static SPECIES: std::sync::OnceLock<Species> = std::sync::OnceLock::new();
pub fn species() -> &'static Species {
    SPECIES.get().expect("a level was built")
}
/// The box of a creature of this size standing at `p`.
pub fn box_at(p: Vec3, width: f32, height: f32) -> Aabb {
    let (hw, h) = (f64::from(width) / 2.0, f64::from(height));
    Aabb::new(Vec3::new(p.x - hw, p.y, p.z - hw), Vec3::new(p.x + hw, p.y + h, p.z + hw))
}
/// An animal of the world's species at (x, z): every number is the species' data.
pub fn spawn_bundle(sp: &Species, x: f64, z: f64, seed: u64) -> impl Bundle {
    let mut rng = Rng(seed | 1);
    (
        Pos(Vec3::new(x, 1.0, z)),
        SpeciesId(sp.id),
        Vel(Vec3::ZERO),
        Yaw((rng.next_u32() % 360) as f32),
        Body { width: sp.width, height: sp.height, step_height: sp.step_height, gravity: sp.gravity, speed: sp.speed },
        Bounds(box_at(Vec3::new(x, 1.0, z), sp.width, sp.height)),
        Cell(u32::MAX),
        Contact { on_ground: false, horizontal: false, fall_distance: 0.0 },
        Drive::default(),
        (
            Sizes { adult: (sp.width, sp.height), baby: sp.baby },
            MoveTo::default(),
            Nav::default(),
            ActiveUntil(100),
            Stroll { walking: false, scheduled: false, tx: 0.0, tz: 0.0, ty: 1, horizontal_range: 10, vertical_range: 7, interval: 60 },
            (Moving, crate::goals::goals_bundle(sp)),
            Health(sp.max_health),
            HurtAt { at: 0, last: 0.0 },
            Frozen(0),
            (FluidState::default(), Air(300), Recheck),
            rng,
        ),
    )
}
/// Spawns an animal of species `sp` (with the extra data of a species that attacks).
pub fn spawn_species(world: &mut World, sp: &Species, x: f64, z: f64, seed: u64) -> Entity {
    let e = world.spawn(spawn_bundle(sp, x, z, seed)).id();
    crate::hostile::equip(world, e, sp);
    crate::farm::equip(world, e, sp);
    e
}
/// An animal of the species the process was built for (see `species`).
pub fn cow_bundle(x: f64, z: f64, seed: u64) -> impl Bundle {
    spawn_bundle(species(), x, z, seed)
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
    // The damage queued since the last tick (a blow, a fall, a hazard) is applied first: its recoil is integrated by
    // this tick's physics, as a mob's hurt precedes its tick.
    s.add_systems((due, apply_damage, block_changes, recompute, player_near, pathfind, follow, path_ended, move_control, fluid_update, breathing, wall_check, travel, reindex, hazard_tick, wall_damage, push, apply_pushes).chain());
    crate::goals::register(&mut s);
    crate::hostile::register(&mut s);
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
pub(crate) fn pathfind(
    cfg: Res<Config>,
    clock: Res<Clock>,
    level: Res<Level>,
    table: Res<SpeciesTable>,
    par: ParallelCommands,
    mut q: Query<(Entity, &Pos, &Body, &Contact, &mut Stroll, &mut Nav, Has<InFluid>, &Vel, &SpeciesId), With<NeedsPath>>,
) {
    let now = clock.0;
    let level = &*level;
    let one = |e: Entity, pos: &Pos, body: &Body, c: &Contact, s: &mut Stroll, nav: &mut Nav, fluid: bool, vel: &Vel, sid: &SpeciesId| {
        let sp = &table.0[sid.0 as usize];
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
        let target = [s.tx.floor() as i32, s.ty, s.tz.floor() as i32];
        // `GroundPathNavigation.canUpdatePath`: no path is made in the air (on the ground, in a liquid, riding).
        let path = (c.on_ground || fluid).then(|| Finder::new(mw, &pm, Evaluator::Walk).find(&[target], max_len, 1, max_visited)).flatten();
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
        q.par_iter_mut().for_each(|(e, p, b, c, mut s, mut n, f, v, i)| one(e, p, b, c, &mut s, &mut n, f, v, i));
    } else {
        for (e, p, b, c, mut s, mut n, f, v, i) in &mut q {
            one(e, p, b, c, &mut s, &mut n, f, v, i);
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
            drive.jump = false;
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
fn move_control_one(level: &Level, pos: &Pos, yaw: &mut Yaw, m: &mut MoveTo, d: &mut Drive, body: &Body, landed: bool) {
    const ARRIVED: f64 = 2.500_000_3e-7_f32 as f64;
    d.jump = false;
    if d.jumping_op {
        // Operation JUMPING: the speed follows the wanted spot's modifier, the heading is kept, and the next
        // wanted spot does not change the operation, until the cow is on the ground (or in a liquid).
        let speed = (m.speed_modifier * body.speed) as f32;
        d.speed = speed;
        d.zza = speed;
        m.active = false;
        if landed {
            d.jumping_op = false;
        }
        return;
    }
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
    // `MoveControl` MOVE_TO: a target above the step height and near makes the cow jump.
    d.jump = yd > f64::from(body.step_height) && xd * xd + zd * zd < f64::from(body.width.max(1.0)) || stands_in_low_block(level, pos);
    d.jumping_op = d.jump;
}
/// The second clause of the `MoveControl` jump: the cow stands in a block with a collision shape that has not
/// reached its top (a cactus, a slab, a carpet): it jumps, unless the block is a door or a fence.
fn stands_in_low_block(level: &Level, pos: &Pos) -> bool {
    let mw = level.mw.0;
    let (bx, by, bz) = (pos.0.x.floor() as i32, pos.0.y.floor() as i32, pos.0.z.floor() as i32);
    let state = mw.world.block(bx, by, bz);
    let top = mw.data.state(state).top;
    if top <= 0.0 {
        return false;
    }
    let name = &mw.facts.get(state).name;
    pos.0.y < f64::from(top) + f64::from(by) && !(name.ends_with("_door") || name.ends_with("fence"))
}
fn move_control(cfg: Res<Config>, level: Res<Level>, mut q: Query<(&Pos, &mut Yaw, &mut MoveTo, &mut Drive, &Body, &Contact, &FluidState), With<Following>>) {
    let landed = |c: &Contact, f: &FluidState| c.on_ground || f.in_water || f.in_lava;
    if cfg.parallel {
        q.par_iter_mut().for_each(|(p, mut y, mut m, mut d, b, c, f)| move_control_one(&level, p, &mut y, &mut m, &mut d, b, landed(c, f)));
    } else {
        for (p, mut y, mut m, mut d, b, c, f) in &mut q {
            move_control_one(&level, p, &mut y, &mut m, &mut d, b, landed(c, f));
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
    // `Entity.move` ends by losing the velocity along the axes that collided (a landing, a ceiling, a wall).
    if vertical {
        vel.0.y = 0.0;
    }
    if differs(wanted.x, moved.x) {
        vel.0.x = 0.0;
    }
    if differs(wanted.z, moved.z) {
        vel.0.z = 0.0;
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
    d: &mut Drive,
    body: &Body,
    bounds: &mut Bounds,
    fs: &FluidState,
    jumping: bool,
    fall_damage: &mut f32,
) -> bool {
    let rest_vy = (0.0 - body.gravity) * f64::from(0.98_f32);
    let in_fluid = fs.in_water || fs.in_lava;
    let jumping = jumping || d.jump;
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
    // `LivingEntity.handleJumping` on the ground: a jump every 10 ticks at most (`noJumpDelay`), the delay reset when
    // the input stops; `jumpFromGround`: the jump power 0.42 (the block's factor is 1 here).
    if d.no_jump_delay > 0 {
        d.no_jump_delay -= 1;
    }
    if !jumping {
        d.no_jump_delay = 0;
    } else if !in_fluid && c.on_ground && d.no_jump_delay == 0 {
        vel.0.y = vel.0.y.max(0.42);
        d.no_jump_delay = 10;
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
    !jumping && c.on_ground && d.zza == 0.0 && vel.0.x == 0.0 && vel.0.z == 0.0 && vel.0.y == rest_vy
}
#[allow(clippy::type_complexity)]
fn travel(
    cfg: Res<Config>,
    level: Res<Level>,
    table: Res<SpeciesTable>,
    par: ParallelCommands,
    mut q: Query<
        (Entity, &mut Pos, &mut Vel, &Yaw, &mut Contact, &mut Drive, &Body, &mut Bounds, &FluidState, Has<Following>, Option<&crate::goals::Jump>, &SpeciesId),
        With<Moving>,
    >,
) {
    let one = |e: Entity, mut p: Mut<Pos>, mut v: Mut<Vel>, y: &Yaw, mut c: Mut<Contact>, mut d: Mut<Drive>, b: &Body, mut bb: Mut<Bounds>, fs: &FluidState, following: bool, jump: Option<&crate::goals::Jump>, sid: &SpeciesId| {
        let sp = &table.0[sid.0 as usize];
        let mut fall = 0.0_f32;
        let jumping = jump.is_some_and(|j| j.0);
        let rest = travel_one(&level, sp, &mut p, &mut v, y, &mut c, &mut d, b, &mut bb, fs, jumping, &mut fall);
        if fall > 0.0 {
            par.command_scope(|mut cmd| {
                cmd.queue(move |w: &mut World| w.resource_mut::<DamageQueue>().add(e, fall, Cause::Fall));
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
        q.par_iter_mut().for_each(|(e, p, v, y, c, d, b, bb, fs, f, j, i)| one(e, p, v, y, c, d, b, bb, fs, f, j, i));
    } else {
        for (e, p, v, y, c, d, b, bb, fs, f, j, i) in &mut q {
            one(e, p, v, y, c, d, b, bb, fs, f, j, i);
        }
    }
}

/// `Entity.updateFluidInteraction` (Horizon's real fluid heights and currents), only for the cows whose box overlaps
/// a column with a fluid.
fn fluid_update(
    cfg: Res<Config>,
    level: Res<Level>,
    table: Res<SpeciesTable>,
    mut q: Query<(&Pos, &Bounds, &mut Vel, &mut FluidState, &mut Contact, &SpeciesId), With<InFluid>>,
) {
    let level = &*level;
    let one = |pos: &Pos, b: &Bounds, v: &mut Vel, fs: &mut FluidState, c: &mut Contact, sid: &SpeciesId| {
        let mw = level.mw.0;
        let f = mw.fluids(b.0, pos.0.y + table.0[sid.0 as usize].eye_height);
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
        q.par_iter_mut().for_each(|(p, b, mut v, mut fs, mut c, i)| one(p, b, &mut v, &mut fs, &mut c, i));
    } else {
        for (p, b, mut v, mut fs, mut c, i) in &mut q {
            one(p, b, &mut v, &mut fs, &mut c, i);
        }
    }
}

/// `LivingEntity.baseTick`'s air: the cows in a fluid, or with air missing, breathe (Horizon's real `breathe` and
/// `drown`): the air runs down while the eyes are in water, refills by 4 outside, and drowning hurts 2.
#[allow(clippy::type_complexity)]
fn breathing(
    level: Res<Level>,
    table: Res<SpeciesTable>,
    mut q: Query<(Entity, &Pos, &FluidState, &mut Air, Has<AirLow>, Has<InFluid>, &SpeciesId), Or<(With<InFluid>, With<AirLow>)>>,
    mut queue: ResMut<DamageQueue>,
    mut commands: Commands,
) {
    let mw = level.mw.0;
    for (e, pos, fs, mut air, low, in_fluid, sid) in &mut q {
        let sp = &table.0[sid.0 as usize];
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
        let breath = horizon_client_lib::breathing::breathe(mw.loop_context, id, sp.name, vanilla);
        if let Some(d) = horizon_client_lib::breathing::drown(mw.loop_context, id, sp.name, &mut air.0, 300, breath, true)
            && let Some(damage) = d.damage
        {
            queue.add(e, damage, Cause::Drown);
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
    table: Res<SpeciesTable>,
    q: Query<(Entity, &Pos, &Body, Has<InWall>, &SpeciesId), With<Recheck>>,
    mut commands: Commands,
) {
    use horizon_core::geometry::{ShapeOp, VoxelShape};
    let mw = level.mw.0;
    for (e, pos, body, was, sid) in &q {
        let sp = &table.0[sid.0 as usize];
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
        queue.add(e, 1.0, Cause::InWall);
    }
}

/// The index follows the boxes that changed (the cows that moved): only a change of column touches it; a change of
/// the hazards under the box adds or takes off the `Hazard` module.
#[allow(clippy::type_complexity)]
pub(crate) fn reindex(
    mut idx: ResMut<Index>,
    mut moves: ResMut<crate::goals::CellMoves>,
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
            moves.0.push(e);
        }
        let bits = hazards.of_box(&b.0);
        let fluid = bits & (HAZ_WATER | HAZ_LAVA) != 0;
        if fluid && !in_fluid {
            // A cow that enters a fluid (placed water, a push) needs its physics at once.
            commands.entity(e).insert((InFluid, Moving));
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

/// `LivingEntity.hurtServer` for the queued hits: within 10 ticks of a hurt only a larger hit counts, by the
/// difference; a cow at 0 health dies (leaves the index and the world). The effects `Mob` attaches to a hit that goes
/// through are applied here, in the same path (read in `mobs.rs` `hurt_server`): the knockback of an entity's hit
/// (0.4 away from the attacker, or a random direction when it has no position, scaled by the species' resistance,
/// a hop on the ground; no knockback for the environmental causes nor for a hit inside the cooldown), the end of
/// love (`Animal.actuallyHurt`), and `noActionTime` back to 0 (`ai::after_hurt`: the stroll goal is allowed again for
/// 100 ticks). The goals read the hit through `Hurts` and `HurtCauses` (Panic).
#[allow(clippy::type_complexity)]
pub(crate) fn apply_damage(
    clock: Res<Clock>,
    table: Res<SpeciesTable>,
    mut queue: ResMut<DamageQueue>,
    mut idx: ResMut<Index>,
    mut wheel: ResMut<Wheel>,
    mut q: Query<(&mut Health, &mut HurtAt, &Cell, &Pos, &mut Vel, &Contact, &mut Rng, &mut ActiveUntil, &mut Stroll, Has<Following>, &SpeciesId)>,
    mut hurts: ResMut<crate::goals::Hurts>,
    mut by: ResMut<crate::hostile::HurtBy>,
    mut causes: ResMut<HurtCauses>,
    mut commands: Commands,
) {
    // The causes of the previous step were read by the goals (they run after this system in the chain).
    causes.0.clear();
    for hit in std::mem::take(&mut queue.0) {
        let e = hit.target;
        let Ok((mut h, mut hurt, cell, pos, mut v, c, mut rng, mut active, mut stroll, following, sid)) = q.get_mut(e) else {
            continue;
        };
        let cooldown = clock.0 < hurt.at + 10 && hurt.at != 0;
        let applied = if cooldown {
            if hit.amount <= hurt.last {
                continue;
            }
            hit.amount - hurt.last
        } else {
            hit.amount
        };
        *hurt = HurtAt { at: clock.0, last: hit.amount };
        h.0 -= applied;
        if h.0 <= 0.0 {
            if cell.0 != u32::MAX {
                idx.remove(e, cell.0);
            }
            commands.entity(e).despawn();
            continue;
        }
        hurts.0.push(e);
        causes.0.push((e, hit.cause));
        if let Some(p) = hit.by {
            by.0.push((e, p));
        }
        // `Animal.actuallyHurt`: damage taken ends love.
        commands.entity(e).remove::<crate::goals::Love>();
        // `ai::after_hurt`: `noActionTime = 0`.
        if active.0 != u64::MAX {
            active.0 = active.0.max(clock.0 + 100);
            if !stroll.scheduled && !stroll.walking {
                // The wait is drawn from a stream of its own: the cow's random belongs to its goals (the target
                // of a panic), which the hit must not shift.
                let mut own = Rng(((e.index().index() as u64) << 32 ^ clock.0 ^ 0x9e37_79b9_7f4a_7c15) | 1);
                arm_stroll(&mut wheel, clock.0, e, &mut stroll, &mut own);
            }
        }
        // `dealDefaultKnockback`: an entity's hit outside the cooldown (the other causes are in `#no_knockback`).
        if hit.cause == Cause::Entity && !cooldown {
            let power = 0.4_f64 * (1.0 - table.0[sid.0 as usize].knockback_resistance);
            if power > 0.0 {
                let (mut dx, mut dz) = hit.from.map_or((0.0, 0.0), |f| (f.0 - pos.0.x, f.1 - pos.0.z));
                while dx * dx + dz * dz < f64::from(1.0e-5_f32) {
                    dx = (rng.next_f64() - rng.next_f64()) * 0.01;
                    dz = (rng.next_f64() - rng.next_f64()) * 0.01;
                }
                let l = (dx * dx + dz * dz).sqrt();
                let (kx, kz) = (dx / l * power, dz / l * power);
                let y = if c.on_ground { (v.0.y / 2.0 + power).min(0.4) } else { v.0.y };
                v.0 = Vec3::new(v.0.x / 2.0 - kx, y, v.0.z / 2.0 - kz);
                commands.entity(e).insert(Moving);
            }
        }
        let _ = following;
    }
}
/// The module of the cows that overlap a hazard (only they are visited): lava ignites 15 s and hurts 4, fire ignites
/// 8 s, a cactus hurts 1, powder snow counts the freezing (1 damage every 40 ticks once frozen for 140), water puts
/// the fire out. The fire's own damage (1 every 20 ticks) is in the timer wheel.
#[allow(clippy::type_complexity)]
fn hazard_tick(
    clock: Res<Clock>,
    level: Res<Level>,
    table: Res<SpeciesTable>,
    mut wheel: ResMut<Wheel>,
    mut queue: ResMut<DamageQueue>,
    mut q: Query<(Entity, &Hazard, &mut Frozen, Option<&Burning>, &Pos, &Bounds, &SpeciesId)>,
    mut commands: Commands,
) {
    let now = clock.0;
    let mw = level.mw.0;
    for (e, h, mut frozen, burning, pos, b, sid) in &mut q {
        let f = h.0;
        // Water and lava as the `Mob` reads them: the fluid height at the cow's box, not the column.
        let (in_water, in_lava) = if f & (HAZ_WATER | HAZ_LAVA) != 0 {
            let r = mw.fluids(b.0, pos.0.y + table.0[sid.0 as usize].eye_height);
            (r.in_water, r.in_lava)
        } else {
            (false, false)
        };
        let lit = f & HAZ_FIRE != 0 || in_lava;
        if in_water {
            if burning.is_some() {
                commands.entity(e).remove::<Burning>();
            }
        } else {
            let seconds = if in_lava { 300 } else if lit { 160 } else { 0 };
            if seconds > 0 {
                let until = now + seconds;
                match burning {
                    None => {
                        // `Entity.baseTick` hurts when the time left is a multiple of 20, the first tick included.
                        wheel.push(now + if seconds % 20 == 0 { 1 } else { 20 }, e, Due::Burn);
                        commands.entity(e).insert(Burning { until });
                    }
                    Some(b) if b.until < until => {
                        commands.entity(e).insert(Burning { until });
                    }
                    _ => {}
                }
            }
        }
        if in_lava {
            queue.add(e, 4.0, Cause::Lava);
        }
        if f & HAZ_FIRE != 0 {
            // `FireBlock.entityInside`: the fire hurts 1 as well as setting alight.
            queue.add(e, 1.0, Cause::InFire);
        }
        if f & HAZ_CACTUS != 0 {
            queue.add(e, 1.0, Cause::Cactus);
        }
        if f & HAZ_SNOW != 0 {
            frozen.0 = (frozen.0 + 1).min(140);
            if frozen.0 >= 140 && now % 40 == 0 {
                queue.add(e, 1.0, Cause::Freeze);
            }
        }
    }
}
/// A path that a block change asked to make again (`PathNavigation.pendingRecompute` and `delayedRecompute`).
#[derive(Component, Default)]
pub struct Recompute;
/// `PathNavigation.recomputePath`: made at once if the last recompute is more than 20 ticks old and the cow can update
/// a path (on the ground or in a liquid); else the old path is kept and the ask is made again every tick.
#[allow(clippy::type_complexity)]
fn recompute(
    clock: Res<Clock>,
    mut q: Query<(Entity, &mut Nav, &Contact, &FluidState), With<Recompute>>,
    mut commands: Commands,
) {
    for (e, mut nav, c, fs) in &mut q {
        if nav.path.as_ref().is_none_or(|p| p.is_done()) {
            nav.delayed = false;
            commands.entity(e).remove::<Recompute>();
        } else if clock.0.saturating_sub(nav.last_recompute) <= 20 || !(c.on_ground || fs.in_water || fs.in_lava) {
            nav.delayed = true;
        } else {
            nav.last_recompute = clock.0;
            nav.path = None;
            nav.delayed = false;
            commands.entity(e).remove::<(Following, Recompute)>().insert(NeedsPath);
        }
    }
}
/// A block was placed or removed (`ServerLevel.sendBlockUpdated`): the world takes it, the hazard column is read again,
/// and only what it can change is woken: the cows whose box, widened by a block, meets the block get their physics
/// (a floor that left), their suffocation and their hazard and fluid modules looked at again (their box counts as
/// changed, so `reindex` re-evaluates them); the cows that follow a path that the change is near the middle of
/// (`PathNavigation.shouldRecomputePath`) ask for a new path to the same spot; nobody else is touched.
#[allow(clippy::type_complexity)]
fn block_changes(
    mut changes: ResMut<BlockChanges>,
    level: Res<Level>,
    mut hazards: ResMut<Hazards>,
    idx: Res<Index>,
    mut sun: ResMut<crate::hostile::SunDirty>,
    mut q: Query<(&mut Bounds, &Pos, &mut Contact)>,
    mut paths: Query<(Entity, &Pos, &Stroll, &mut Nav), With<Following>>,
    mut commands: Commands,
) {
    if changes.0.is_empty() {
        return;
    }
    let mw = level.mw.0;
    for (x, y, z, name) in std::mem::take(&mut changes.0) {
        let Some(state) = mw.registry.default_state(name) else { continue };
        // SAFETY: this system runs alone in the chain, no reference to the blocks is alive meanwhile.
        unsafe {
            let world = &mut *level.blocks.0;
            let _ = world.set(x, y, z, state);
            world.path_type_block_updated(horizon_core::position::BlockPos::new(x, y, z));
        }
        hazards.set(x, y, z, hazard_bit(name));
        let cell = Aabb::new(Vec3::new(f64::from(x), f64::from(y), f64::from(z)), Vec3::new(f64::from(x + 1), f64::from(y + 1), f64::from(z + 1)));
        let reach = cell.inflate(Vec3::new(1.0, 1.0, 1.0));
        let mut near: Vec<Entity> = Vec::new();
        idx.for_near(f64::from(x) + 0.5, f64::from(z) + 0.5, |e| near.push(e));
        // The sky above a column changed: whoever stands in it looks again at the sun.
        sun.0.extend(near.iter().copied());
        for e in near {
            if let Ok((mut b, _, mut c)) = q.get_mut(e)
                && b.0.intersects(reach)
            {
                b.set_changed();
                // The ground it stood on is no longer known: the next step sweeps for real instead of resting.
                c.on_ground = false;
                commands.entity(e).insert((Moving, Recheck));
            }
        }
        // The paths that run near the change.
        for (e, pos, s, mut nav) in &mut paths {
            let Some(path) = nav.path.as_ref().filter(|p| !p.is_done()) else { continue };
            let Some(end) = path.nodes.last() else { continue };
            let middle = Vec3::new(
                (f64::from(end.x) + pos.0.x) / 2.0,
                (f64::from(end.y) + pos.0.y) / 2.0,
                (f64::from(end.z) + pos.0.z) / 2.0,
            );
            let remaining = (path.nodes.len() - path.next) as f64;
            let c = Vec3::new(f64::from(x) + 0.5, f64::from(y) + 0.5, f64::from(z) + 0.5);
            if c.distance_squared(middle) < remaining * remaining && s.walking && !nav.delayed {
                commands.entity(e).insert(Recompute);
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
    mut sun: ResMut<crate::hostile::SunFired>,
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
            Due::Sun => sun.0.push(e),
            Due::Burn => {
                if let Ok((e2, burning)) = burners.get(e) {
                    if clock.0 < burning.until {
                        queue.add(e2, 1.0, Cause::OnFire);
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
        Self::with_mix(n, spacing, sleep, parallel, seed, hazards, 0)
    }
    /// A mixed herd: every `other_every`-th animal is of the second species (the pig), the others of the first (the
    /// cow); 0: all of the process's default species.
    pub fn with_mix(n: usize, spacing: f64, sleep: bool, parallel: bool, seed: u64, hazards: bool, other_every: usize) -> Self {
        Self::build(n, spacing, sleep, parallel, seed, hazards, other_every, None)
    }
    /// An empty walled slab of `half` blocks each way.
    pub fn with_slab(half: i32, sleep: bool, hazards: bool) -> Self {
        Self::build(0, 1.0, sleep, false, 5, hazards, 0, Some(half))
    }
    #[allow(clippy::too_many_arguments)]
    fn build(n: usize, spacing: f64, sleep: bool, parallel: bool, seed: u64, hazards: bool, other_every: usize, slab: Option<i32>) -> Self {
        let side = (n as f64).sqrt().ceil() as i32;
        let half = slab.unwrap_or(((f64::from(side) * spacing / 2.0) as i32 + 6).max(20));
        let mut world = World::new();
        let (level, species, _kind) = build_level(half, hazards);
        world.insert_resource(Hazards::build(&level));
        world.insert_resource(DamageQueue::default());
        world.insert_resource(HurtCauses::default());
        world.insert_resource(BlockChanges::default());
        world.insert_resource(level);
        let _ = species;
        world.insert_resource(species_table());
        world.insert_resource(Config { sleep, parallel });
        world.insert_resource(Clock::default());
        world.insert_resource(Players::default());
        world.insert_resource(Wheel::default());
        crate::goals::insert_resources(&mut world);
        crate::hostile::insert_resources(&mut world);
        world.insert_resource(Index::new(-f64::from(half), (2 * half / 2) as usize + 2));
        let mut rng = Rng(seed | 1);
        for i in 0..n {
            let (cx, cz) = ((i as i32 % side) as f64, (i as i32 / side) as f64);
            let o = -f64::from(side) * spacing / 2.0;
            let s = u64::from(rng.next_u32()) << 32 | u64::from(rng.next_u32());
            let table = species_table();
            let sp = if other_every > 0 { table.0[usize::from(i % other_every == 0)] } else { species };
            spawn_species(&mut world, &sp, o + cx * spacing + 0.5, o + cz * spacing + 0.5, s);
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
    /// The players as `Online` knows them (ids from 1, at y = 1, nothing in hand), unkillable: for the hunting benches.
    pub fn set_online(&mut self, players: &[(f64, f64)]) {
        self.world.resource_mut::<crate::hostile::PlayerState>().immortal = true;
        for (i, p) in players.iter().enumerate() {
            self.world.resource_mut::<crate::goals::Online>().set(crate::goals::Player { id: i as u32 + 1, x: p.0, y: 1.0, z: p.1, main: 0, off: 0 });
        }
    }
    /// Day or night for the monsters that burn in daylight (`monsters_burn`): every one of them looks at the sun again.
    pub fn set_day(&mut self, day: bool) {
        self.world.resource_mut::<crate::hostile::Daylight>().0 = day;
        let all: Vec<Entity> = self.world.query_filtered::<Entity, With<crate::hostile::BurnsInDay>>().iter(&self.world).collect();
        self.world.resource_mut::<crate::hostile::SunDirty>().0.extend(all);
    }
    /// A block placed or removed (`name`: `minecraft:stone`, `minecraft:air`, `minecraft:lava`...): taken at the next step.
    pub fn set_block(&mut self, x: i32, y: i32, z: i32, name: &'static str) {
        self.world.resource_mut::<BlockChanges>().0.push((x, y, z, name));
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
        assert!(herd.world.get::<Body>(baby).unwrap().width < species().width * 0.75);
        for _ in 0..40 {
            herd.tick();
        }
        assert!(herd.world.get::<Body>(baby).unwrap().width > species().width * 0.9);
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
            herd.world.get_mut::<Bounds>(e).unwrap().0 = box_at(Vec3::new(x, y, z), species().width, species().height);
            herd.world.entity_mut(e).insert(ActiveUntil(0));
            e
        };
        let health = |herd: &Herd, e: Entity| herd.world.get::<Health>(e).map(|h| h.0);
        let (hurt, dies) = (cow(&mut herd, 5.5, 1.0 + (0.8 * f64::from(species().max_health)).floor() + 3.0, 5.5), cow(&mut herd, 8.5, 1.0 + f64::from(species().max_health) + 5.0, 5.5));
        let lava = cow(&mut herd, -47.0, 1.0, -47.0);
        let fire = cow(&mut herd, -47.0, 1.0, -39.0);
        let snow = cow(&mut herd, -31.0, 1.0, -47.0);
        let cactus = cow(&mut herd, -39.0, 1.0, -39.0);
        let pond = cow(&mut herd, -39.0, 3.0, -47.0);
        // No float goal for this cow (the Mob case has none either): it sinks and drowns.
        herd.world.entity_mut(pond).remove::<crate::goals::Jump>();
        let wall = cow(&mut herd, 20.5, 1.0, 20.5);
        // The eyes inside the stone floor (y = 0.7), whatever the height of the species.
        herd.teleport(wall, Vec3::new(20.5, 0.7 - species().eye_height, 20.5));
        let show = |herd: &Herd, t: u32| {
            let h = |e: Entity| health(herd, e).map_or("dead".to_string(), |h| format!("{h}"));
            eprintln!("tick {t}: fall 11: {}, fall 39: {}, lava: {}, fire: {}, powder snow: {}, cactus: {}, pond: {}, in wall: {}", h(hurt), h(dies), h(lava), h(fire), h(snow), h(cactus), h(pond), h(wall));
        };
        for _ in 0..40 {
            herd.tick();
        }
        show(&herd, 40);
        // The fall: 11 blocks hurt by 8 (ceil(11 - 3)); 39 kill.
        assert!(health(&herd, hurt).is_some_and(|h| h > 0.0 && h < species().max_health), "{:?}", health(&herd, hurt));
        assert!(health(&herd, dies).is_none(), "the cow that fell 39 blocks lives");
        assert!(health(&herd, cactus).is_some_and(|h| h < species().max_health), "the cactus did not hurt");
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
        assert!(health(&herd, snow).is_some_and(|h| h < species().max_health), "the powder snow: {:?}", health(&herd, snow));
        assert!(herd.world.get::<Burning>(fire).is_some() || health(&herd, fire).is_none());
        // The pond is 2 blocks deep: only an animal whose eyes sit lower than 1.4 blocks goes under (a zombie's are at 1.74).
        if species().eye_height <= 1.4 {
            assert!(health(&herd, pond).is_none_or(|h| h < species().max_health), "the cow in the pond did not drown");
        }
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
        let speed: f32 = std::env::var("SPEED").ok().and_then(|v| v.parse::<f32>().ok()).unwrap_or(1.0) * 0.2;
        herd.world.get_mut::<Pos>(e).unwrap().0 = Vec3::new(-53.0, 1.0, -20.0);
        herd.world.get_mut::<Bounds>(e).unwrap().0 = box_at(Vec3::new(-53.0, 1.0, -20.0), species().width, species().height);
        herd.world.get_mut::<Yaw>(e).unwrap().0 = -90.0;
        herd.world.get_mut::<Drive>(e).unwrap().speed = speed;
        herd.world.get_mut::<Drive>(e).unwrap().zza = speed;
        let mut last = -53.0;
        for t in 1..=100 {
            herd.tick();
            let Some(p) = herd.world.get::<Pos>(e).map(|p| p.0) else {
                eprintln!("ecs walk t={t:3}: the cow is gone (last x = {last:.2})");
                break;
            };
            let x = p.x;
            if t % 10 == 0 {
                eprintln!("  health {:?} hazard {:?} y {:.2}", herd.world.get::<Health>(e).map(|h| h.0), herd.world.get::<Hazard>(e).map(|h| h.0), p.y);
            }
            if [10, 20, 30, 40, 60, 80, 100].contains(&t) {
                eprintln!("ecs walk t={t:3} x={x:.4} step={:.4} vel.x={:.4}", x - last, herd.world.get::<Vel>(e).unwrap().0.x);
            }
            last = x;
        }
    }

    /// A cow that keeps jumping under a low ceiling, tick by tick (to compare with `Mob`: the jump delay).
    #[test]
    #[ignore = "trace"]
    fn trace_a_cow_jumping_under_a_ceiling() {
        let mut herd = Herd::with_hazards(1, 100.0, true, false, 5, true);
        let e = herd.world.spawn(cow_bundle(-19.0, -19.0, 77)).id();
        herd.world.entity_mut(e).insert(ActiveUntil(0));
        herd.world.entity_mut(e).remove::<crate::goals::Jump>();
        herd.world.get_mut::<Drive>(e).unwrap().jump = true;
        let ys: Vec<String> = (1..=40)
            .map(|_| {
                herd.tick();
                format!("{:.2}", herd.world.get::<Pos>(e).unwrap().0.y)
            })
            .collect();
        eprintln!("ecs jump y: {}", ys.join(" "));
    }

    /// Cows and pigs in one world: each keeps its own values (size, panic speed), is tempted by its own item, the
    /// two kinds push each other, and they do not breed with each other.
    #[test]
    fn cows_and_pigs_share_a_world_and_keep_their_own_values() {
        use crate::goals::{CARROT, Online, Panic, Player, Tempting, WHEAT, feed};
        if std::env::var_os("HORIZON_TEST_RESOURCES").is_none() {
            return;
        }
        let mut herd = Herd::with_mix(16, 3.0, true, false, 5, false, 2);
        let table = herd.world.resource::<SpeciesTable>().0.clone();
        let (cow, pig) = (table[0], table[1]);
        assert_ne!((cow.width, cow.height), (pig.width, pig.height));
        let all: Vec<(Entity, u8)> =
            herd.world.query::<(Entity, &SpeciesId)>().iter(&herd.world).map(|(e, i)| (e, i.0)).collect();
        assert!(all.iter().filter(|a| a.1 == 0).count() == 8 && all.iter().filter(|a| a.1 == 1).count() == 8);
        for &(e, id) in &all {
            let sp = table[id as usize];
            let b = herd.world.get::<Body>(e).unwrap();
            assert_eq!((b.width, b.height, b.speed), (sp.width, sp.height, sp.speed), "{}", sp.name);
            assert_eq!(herd.world.get::<Panic>(e).unwrap().speed, sp.goals.panic);
        }
        // A player with a carrot in the middle: the pigs are tempted, the cows are not; then with wheat, the reverse.
        let tempted = |herd: &mut Herd, id: u8| {
            all.iter().filter(|a| a.1 == id && herd.world.get::<Tempting>(a.0).is_some()).count()
        };
        herd.world.resource_mut::<Online>().set(Player { id: 1, x: 0.0, y: 1.0, z: 0.0, main: CARROT, off: 0 });
        for _ in 0..8 {
            herd.tick();
        }
        let (cows_c, pigs_c) = (tempted(&mut herd, 0), tempted(&mut herd, 1));
        herd.world.resource_mut::<Online>().set(Player { id: 1, x: 0.0, y: 1.0, z: 0.0, main: WHEAT, off: 0 });
        for _ in 0..8 {
            herd.tick();
        }
        let (cows_w, pigs_w) = (tempted(&mut herd, 0), tempted(&mut herd, 1));
        eprintln!("carrot: cows {cows_c} pigs {pigs_c}; wheat: cows {cows_w} pigs {pigs_w}");
        assert!(cows_c == 0 && pigs_c > 0 && pigs_w == 0 && cows_w > 0);
        // A cow and a pig in love next to each other: no baby; two cows in love: a baby.
        let mut herd = Herd::with_mix(0, 3.0, true, false, 5, false, 0);
        let spawn = |herd: &mut Herd, sp: &Species, x: f64| {
            let e = herd.world.spawn(spawn_bundle(sp, x, 0.5, 9 + x as u64)).id();
            herd.world.get_mut::<ActiveUntil>(e).unwrap().0 = 0;
            e
        };
        let (c1, p1, c2, c3) = (spawn(&mut herd, &cow, 0.5), spawn(&mut herd, &pig, 2.5), spawn(&mut herd, &cow, -10.5), spawn(&mut herd, &cow, -8.5));
        for e in [c1, p1, c2, c3] {
            assert!(feed(&mut herd.world, e));
        }
        let count = |herd: &mut Herd| herd.world.query::<&SpeciesId>().iter(&herd.world).count();
        for _ in 0..300 {
            herd.tick();
        }
        let babies: Vec<u8> = herd
            .world
            .query_filtered::<&SpeciesId, With<GrowAt>>()
            .iter(&herd.world)
            .map(|i| i.0)
            .collect();
        assert_eq!(count(&mut herd), 5, "one baby, from the two cows only");
        assert_eq!(babies, vec![0], "a cow calf");
    }

    /// Block changes as events: a floor removed wakes the cow on it only; a stone put at head height suffocates; lava
    /// put into a cow's cell adds the hazard module; a path is dropped only by a change near its middle.
    #[test]
    fn block_changes_wake_only_what_they_touch() {
        if std::env::var_os("HORIZON_TEST_RESOURCES").is_none() {
            return;
        }
        let mut herd = Herd::with_hazards(1, 100.0, true, false, 5, false);
        let mut spawn = |herd: &mut Herd, x: f64, z: f64| {
            let e = herd.world.spawn(cow_bundle(x, z, 3 + x as u64)).id();
            herd.world.get_mut::<ActiveUntil>(e).unwrap().0 = 0;
            e
        };
        let (a, far, c, d) = (spawn(&mut herd, 10.5, 10.5), spawn(&mut herd, 30.5, 10.5), spawn(&mut herd, 20.5, 20.5), spawn(&mut herd, -20.5, 20.5));
        for _ in 0..10 {
            herd.tick();
        }
        let moving = |herd: &Herd, e: Entity| herd.world.get::<Moving>(e).is_some();
        assert!(!moving(&herd, a) && !moving(&herd, far), "the cows are not at rest");
        let y0 = herd.world.get::<Pos>(a).unwrap().0.y;
        // The floor under `a` is removed: it wakes and falls a block; the cow far away is not touched.
        herd.set_block(10, 0, 10, "minecraft:air");
        herd.tick();
        assert!(moving(&herd, a) && !moving(&herd, far));
        for _ in 0..6 {
            herd.tick();
        }
        let y1 = herd.world.get::<Pos>(a).unwrap().0.y;
        assert!((y0 - y1 - 1.0).abs() < 1.0e-6 && !moving(&herd, a), "{y0} -> {y1}");
        // A stone at head height: suffocation by the event.
        // A stone in the block of the eyes: the head is in a wall.
        let eye_y = (1.0 + species().eye_height).floor() as i32;
        herd.set_block(20, eye_y, 20, "minecraft:stone");
        for _ in 0..15 {
            herd.tick();
        }
        assert!(herd.world.get::<InWall>(c).is_some() && herd.world.get::<Health>(c).unwrap().0 < species().max_health);
        // Lava put into the cell of a resting cow: its hazard module appears and it burns to death.
        herd.set_block(-21, 1, 20, "minecraft:lava");
        herd.tick();
        assert!(herd.world.get::<Hazard>(d).is_some_and(|h| h.0 & HAZ_LAVA != 0));
        for _ in 0..60 {
            herd.tick();
        }
        assert!(herd.world.get::<Health>(d).is_none(), "the cow in lava lives");
        assert!(herd.world.get::<Health>(far).unwrap().0 == species().max_health && !moving(&herd, far));
        // A path is dropped by a change near its middle only.
        let walker = {
            let e = spawn(&mut herd, -30.5, -20.5);
            // A new animal has no known ground until its first sweep: it cannot update a path before.
            herd.tick();
            herd.tick();
            herd.world.get_mut::<Stroll>(e).unwrap().walking = true;
            herd.world.get_mut::<Stroll>(e).unwrap().tx = -10.5;
            herd.world.get_mut::<Stroll>(e).unwrap().tz = -20.5;
            herd.world.entity_mut(e).insert(NeedsPath);
            e
        };
        for _ in 0..4 {
            herd.tick();
        }
        let nodes = |herd: &Herd| herd.world.get::<Nav>(walker).unwrap().path.as_ref().map(|p| p.nodes.len());
        let before = nodes(&herd).expect("a path");
        herd.set_block(45, 1, 45, "minecraft:stone");
        herd.tick();
        assert_eq!(nodes(&herd).map(|n| n + 0), Some(before), "a far change dropped the path");
        let target_before = herd.world.get::<Nav>(walker).unwrap().path.as_ref().unwrap().nodes.clone();
        herd.set_block(-20, 1, -21, "minecraft:stone");
        herd.set_block(-20, 2, -21, "minecraft:stone");
        herd.tick();
        assert!(herd.world.get::<NeedsPath>(walker).is_some() || herd.world.get::<Nav>(walker).unwrap().path.as_ref().is_some_and(|p| p.nodes != target_before));
        for _ in 0..3 {
            herd.tick();
        }
        let new_nodes = herd.world.get::<Nav>(walker).unwrap().path.as_ref().map(|p| p.nodes.clone());
        assert!(new_nodes.clone().is_some_and(|n| n.iter().all(|p| !(p.x == -20 && p.y == 1 && p.z == -21))), "the new path crosses the stone");
        // Less than 20 ticks after that recompute: the next change on the path waits (the old path is kept).
        let nodes_now = new_nodes.unwrap();
        let mid = nodes_now[nodes_now.len() * 2 / 3];
        let (r, remaining) = (herd.world.get::<Nav>(walker).unwrap().last_recompute, herd.world.resource::<Clock>().0);
        assert!(remaining - r < 15, "{r} {remaining}");
        herd.set_block(mid.x, mid.y, mid.z, "minecraft:stone");
        herd.set_block(mid.x, mid.y + 1, mid.z, "minecraft:stone");
        herd.tick();
        herd.tick();
        assert!(herd.world.get::<Nav>(walker).unwrap().delayed, "the recompute is delayed");
        assert_eq!(herd.world.get::<Nav>(walker).unwrap().path.as_ref().map(|p| p.nodes.clone()), Some(nodes_now.clone()));
        for _ in 0..25 {
            herd.tick();
        }
        let after = herd.world.get::<Nav>(walker).unwrap();
        assert!(!after.delayed && after.last_recompute > r + 20, "recomputed after the delay");
    }

    /// Water placed and removed at any height: `InFluid` and the float, fire put out, and a hazard high above the floor.
    #[test]
    fn water_placed_floats_extinguishes_and_works_at_height() {
        if std::env::var_os("HORIZON_TEST_RESOURCES").is_none() {
            return;
        }
        let mut herd = Herd::with_hazards(0, 100.0, true, false, 5, false);
        let mut spawn = |herd: &mut Herd, x: f64, z: f64| {
            let e = herd.world.spawn(cow_bundle(x, z, 3 + x as u64)).id();
            herd.world.get_mut::<ActiveUntil>(e).unwrap().0 = 0;
            e
        };
        let (swimmer, burner, high) = (spawn(&mut herd, 10.5, 10.5), spawn(&mut herd, -10.5, 10.5), spawn(&mut herd, 10.5, -10.5));
        for _ in 0..5 {
            herd.tick();
        }
        // Water around and over the swimmer (the floor is at y = 0, the cow stands in y = 1).
        for x in 9..=11 {
            for z in 9..=11 {
                for y in 1..=3 {
                    herd.set_block(x, y, z, "minecraft:water");
                }
            }
        }
        for _ in 0..30 {
            herd.tick();
        }
        assert!(herd.world.get::<InFluid>(swimmer).is_some());
        let y = herd.world.get::<Pos>(swimmer).unwrap().0.y;
        // Only a species with the float goal rises (a zombie has none: it stays on the bottom).
        if species().goals.slots.iter().any(|s| s.0 == crate::goals::Kind::Float) {
            assert!(y > 1.5, "the cow floats up: y = {y}");
        }
        // Fire in the cell of the second cow, then water over it: the burning stops.
        herd.set_block(-11, 1, 10, "minecraft:fire");
        for _ in 0..3 {
            herd.tick();
        }
        assert!(herd.world.get::<Burning>(burner).is_some(), "burning in fire");
        // Water in the column but above the cow does not put it out; the first hit of the fire is on the first tick.
        herd.set_block(-11, 3, 10, "minecraft:water");
        for _ in 0..3 {
            herd.tick();
        }
        assert!(herd.world.get::<Burning>(burner).is_some(), "water above the cow does not extinguish");
        herd.set_block(-11, 1, 10, "minecraft:water");
        herd.set_block(-10, 1, 10, "minecraft:water");
        for _ in 0..3 {
            herd.tick();
        }
        assert!(herd.world.get::<Burning>(burner).is_none(), "water puts the fire out");
        // The water leaves: the swimmer is no longer in a fluid.
        for x in 9..=11 {
            for z in 9..=11 {
                for y in 1..=3 {
                    herd.set_block(x, y, z, "minecraft:air");
                }
            }
        }
        for _ in 0..40 {
            herd.tick();
        }
        assert!(herd.world.get::<InFluid>(swimmer).is_none(), "water removed");
        assert!(herd.world.get::<Pos>(swimmer).unwrap().0.y < 1.01, "back on the floor");
        // A hazard 4 blocks up (above the generated floor), then the cow is lifted into it: only then it is hurt.
        herd.set_block(10, 6, -11, "minecraft:lava");
        herd.set_block(10, 6, -10, "minecraft:lava");
        for _ in 0..5 {
            herd.tick();
        }
        assert!(herd.world.get::<Hazard>(high).is_none(), "the lava is out of reach");
        let up = Vec3::new(10.5, 6.0, -10.5);
        herd.world.get_mut::<Pos>(high).unwrap().0 = up;
        herd.world.get_mut::<Bounds>(high).unwrap().0 = box_at(up, species().width, species().height);
        herd.tick();
        assert!(herd.world.get::<Hazard>(high).is_some_and(|h| h.0 & HAZ_LAVA != 0), "lava at height 6");
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
