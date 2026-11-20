//! A cow as data components and batch systems (palier 1): the stroll goal (a geometric wait with the law of
//! `nextInt(60) == 0` each tick, then `LandRandomPos` 10 x 7), the move control, `LivingEntity.travel` on the
//! ground with Horizon's real block collision sweep (`SceneCollisions::resolve_in_border_with`), and sleeping.
//! Species data (size, step, speed, stroll range) are component values, not branches.
use bevy_ecs::{
    entity_disabling::Disabled,
    prelude::*,
    query::Allow,
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
    /// The downward speed is multiplied by it in the air (1: none; the hen's `slow_fall`).
    pub fall_factor: f64,
    /// `Strider`: lava holds it up (the collision stands it on the lava surface).
    pub lava_walker: bool,
    /// The motion of an object (`ItemEntity.tick`: the gravity before the move, the drags of the air and the ground, the bounce, the water
    /// and the lava slowing it) instead of a creature's (`LivingEntity.travel`).
    pub object: bool,
    /// With `object`: the motion of an experience orb (`ExperienceOrb.tick`) instead of an item's.
    pub orb: bool,
}
/// The world box of the cow: persistent data, translated by the move, changed by growth; nothing rebuilds it.
#[derive(Component, Clone, Copy, Debug)]
pub struct Bounds(pub Aabb);
/// The spot an animal is bound to (`Mob.extra.home`: a phantom's anchor, a vex's origin).
#[derive(Component, Clone, Copy, Debug)]
pub struct Anchor(pub [i32; 3]);
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
    /// The vertical input (`yya`: a flier's, from `FlyingMoveControl`).
    pub yya: f32,
    /// `Entity.setNoGravity` of a flier while its move control has a spot to go to (`locomotion/fly.rs`).
    pub no_gravity: bool,
    /// The sideways input (`xxa`).
    pub xxa: f32,
    /// The level plus one of a levitation effect (0: none), read by the ground travel.
    pub levitation: i32,
    pub speed: f32,
    /// The move control asks for a jump (`MoveControl`: the target is above the step height and near).
    pub jump: bool,
    /// The jump input (a goal sets it: `FloatGoal` keeps it while the cow is in deep water).
    pub jumping: bool,
    /// `Mob.noJumpDelay`: ticks before the cow may jump again (10 after a jump, 0 when the input stops).
    pub no_jump_delay: i32,
    /// `MoveControl.Operation.JUMPING`: after a jump the move control only keeps the speed until the cow lands.
    pub jumping_op: bool,
    /// A wall climber (`walk::Params.climber`: a spider): pressed against a wall it climbs it. `climbing` is `Spider.climbing`: the contact of
    /// the last tick (`locomotion/climb.rs`).
    pub climber: bool,
    pub climbing: bool,
    /// `LivingEntity.shouldDiscardFriction`: the velocity keeps its horizontal part and takes the vertical one without drag while it is set (a
    /// long jump of a frog or a goat: the goal sets and clears it).
    pub discard_friction: bool,
    /// `Mob.xRot` of a swimmer steered by `SmoothSwimmingMoveControl` (the pitch the inputs follow).
    pub pitch: f32,
    /// `LivingEntity.getFlyingSpeed` of a mount its player steers: a tenth of its speed (`mount.rs`); none: 0,02.
    pub air_speed: Option<f32>,
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
    /// The chance of a plain spot instead of a land one (`WaterAvoidingRandomStrollGoal.probability`: 0.001; the snow golem's 1.0000001e-5).
    pub plain: f32,
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
    /// The searches the path finder ran for this animal (a counter for the benches).
    pub searches: u32,
    /// `PathNavigation.timeLastRecompute` and `delayedRecompute`.
    pub last_recompute: u64,
    pub delayed: bool,
    /// `createPath`'s accuracy: how close to the target block the path may end (1 for a spot, 0 for an entity).
    pub reach: i32,
    /// `Brain.CANT_REACH_WALK_TARGET_SINCE`: the own tick of the first path since the last one that reached its target which did not (0: none).
    pub cant_reach_since: u64,
}
impl Default for Nav {
    fn default() -> Self {
        Self { path: None, last_check: 0, last_pos: Vec3::ZERO, speed: 1.0, searches: 0, last_recompute: 0, delayed: false, reach: 1, cant_reach_since: 0 }
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
#[component(on_add = crate::locomotion::hop::following_started)]
pub struct Following;
/// The physics of the cow is active (it moves, falls, or was just pushed): `travel` visits only these, and takes
/// the marker off when the cow is back at rest.
#[derive(Component, Clone, Copy, Debug)]
#[cfg_attr(feature = "sparse", component(storage = "SparseSet"))]
#[component(on_add = crate::cow::moving_added)]
pub struct Moving;
/// `Mob.noActionTime` as a deadline: the stroll goal is refused from this tick on (100 ticks after a player was
/// last within 32 blocks, or the cow was hurt).
#[derive(Component, Clone, Copy, Debug)]
pub struct ActiveUntil(pub u64);
/// The players of the bench: positions (x, z). `dirty` is the event: a player moved, came or left.
#[derive(Resource, Default)]
pub struct Players {
    pub list: Vec<(f64, f64)>,
    pub(crate) prev: Vec<(f64, f64)>,
    pub dirty: bool,
    /// The players by bin of 32 blocks (side `n`), rebuilt when one moves, the places they had before, and the
    /// cells the last move touched (scratch buffers kept between ticks).
    bins: Vec<Vec<(f64, f64)>>,
    n: i64,
    old: Vec<(f64, f64)>,
    candidates: Vec<usize>,
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
/// `LivingEntity.getMaxHealth`, for the bars and the regeneration that read it.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct MaxHealth(pub f32);
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
/// `Entity.igniteForSeconds`: on fire for `ticks` more (the later end wins); the first burn comes with the wheel as for a fire under it.
pub fn set_on_fire(w: &mut World, e: Entity, ticks: u64) {
    if w.get::<Health>(e).is_none() {
        return;
    }
    let now = now_of(w, e);
    let until = now + ticks;
    match w.get::<Burning>(e).map(|b| b.until) {
        None => {
            let at = w.resource::<Clock>().at(e, now + if ticks % 20 == 0 { 1 } else { 20 });
            w.resource_mut::<Wheel>().push(at, e, Due::Burn);
            w.entity_mut(e).insert(Burning { until });
            w.resource_mut::<crate::goals::Events>().ignited.push(e);
        }
        Some(old) if old < until => {
            w.entity_mut(e).insert(Burning { until });
        }
        _ => {}
    }
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
    /// Who did it (a player of the bench's `Online` list, or a creature), for the target of a `HurtByTarget`.
    pub by: Option<crate::targeting::Who>,
    /// The blow is the attacker's own (not a projectile it shot).
    pub direct: bool,
}
impl DamageQueue {
    pub fn add(&mut self, target: Entity, amount: f32, cause: Cause) {
        self.0.push(Hit { target, amount, cause, from: None, by: None, direct: false });
    }
    pub fn add_by(&mut self, target: Entity, amount: f32, from: (f64, f64), player: u32) {
        self.0.push(Hit { target, amount, cause: Cause::Entity, from: Some(from), by: Some(crate::targeting::Who::Player(player)), direct: true });
    }
    /// A blow of a creature standing at `from`: the victim may take it as its target.
    pub fn add_by_entity(&mut self, target: Entity, amount: f32, from: (f64, f64), attacker: Entity) {
        self.0.push(Hit { target, amount, cause: Cause::Entity, from: Some(from), by: Some(crate::targeting::Who::Entity(attacker)), direct: true });
    }
    /// A blow of `who` through a projectile: the victim may take `who` as its target, but the blow is not direct.
    pub fn add_via(&mut self, target: Entity, amount: f32, from: (f64, f64), who: crate::targeting::Who) {
        self.0.push(Hit { target, amount, cause: Cause::Entity, from: Some(from), by: Some(who), direct: false });
    }
    pub fn add_from(&mut self, target: Entity, amount: f32, cause: Cause, from: (f64, f64)) {
        self.0.push(Hit { target, amount, cause, from: Some(from), by: None, direct: false });
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
    /// `Entity.checkBelowWorld`: 4 a tick under the world (`DamageTypes.OUT_OF_WORLD`).
    OutOfWorld,
    InWall,
    /// `cramming`: too many entities in one place (`LivingEntity.pushEntities`); not a panic cause, no knockback.
    Cramming,
    /// A hit by an entity (a player): `Panic` runs from it by the attacker rule, not the environmental list.
    Entity,
    /// Poison and instant damage (`DamageTypes.MAGIC`).
    Magic,
    /// `DamageTypes.WITHER`.
    Wither,
    /// An explosion (`explosion.rs`).
    Blast,
}
/// The causes of the hurts of this step, in the order of `Hurts` (read by the goals that depend on the damage type).
#[derive(Resource, Default)]
pub struct HurtCauses(pub Vec<(Entity, Cause)>);
/// The block changes waiting for the next step: (x, y, z, block name).
#[derive(Resource, Default)]
pub struct BlockChanges(pub Vec<(i32, i32, i32, &'static str)>);
/// The block changes WITH properties waiting for the next step: (x, y, z, block name, properties): `blocks::set_state`.
#[derive(Resource, Default)]
pub struct StateChanges(pub Vec<(i32, i32, i32, &'static str, &'static [(&'static str, &'static str)])>, pub Vec<(i32, i32, i32, horizon_world::registry::BlockStateId)>);
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
    /// The cells that hold a turtle egg (what the egg hunters look for): a search that has none in its zone does not walk it.
    eggs: std::collections::BTreeSet<[i32; 3]>,
}
const EGG: &str = "minecraft:turtle_egg";
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
        let mut h = Self { origin, width, cols: vec![0; width * width], pool: Vec::new(), eggs: Default::default() };
        for x in 0..width {
            for z in 0..width {
                for y in -2..=BUILD_TOP {
                    let name = &mw.facts.get(mw.world.block(x as i32 + origin, y, z as i32 + origin)).name;
                    if &**name == EGG {
                        h.eggs.insert([x as i32 + origin, y, z as i32 + origin]);
                    }
                    let bit = hazard_bit(name);
                    if bit != 0 {
                        h.set(x as i32 + origin, y, z as i32 + origin, bit);
                    }
                }
            }
        }
        h
    }
    /// The world holds no turtle egg at all.
    pub fn eggs_empty(&self) -> bool {
        self.eggs.is_empty()
    }
    /// A turtle egg lies in the box `lo..=hi` (cells).
    pub fn egg_in(&self, lo: [i32; 3], hi: [i32; 3]) -> bool {
        self.eggs.range([lo[0], i32::MIN, i32::MIN]..=[hi[0], i32::MAX, i32::MAX]).any(|p| (lo[1]..=hi[1]).contains(&p[1]) && (lo[2]..=hi[2]).contains(&p[2]))
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
/// An object (an item on the ground, an experience orb) neither pushes nor is pushed (`Entity.isPushable`): `push` does not visit it.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct PushFree;
/// A baby grows up at this tick (it is in the timer wheel then; nothing counts its age every tick).
#[derive(Component, Clone, Copy, Debug)]
#[component(on_insert = watch_in::<WATCH_FOLLOW_PARENT>, on_discard = watch_out::<WATCH_FOLLOW_PARENT>)]
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
/// The tick counter of the world (advanced by `due`) and the time each entity has not lived: `.0` is the WORLD's time (the
/// timer wheel, the day, the log of the hits); an entity's own time is `of(e)`: the world's minus what it spent
/// asleep (`zone.rs`), as a `Mob`'s `tickCount` does not count the ticks it is not ticked. Every deadline and every date a
/// module or the frame keeps on an entity is in its OWN time; the wheel is in the world's (`at`, `arm`).
#[derive(Resource, Default)]
pub struct Clock(pub u64, Vec<u64>);
impl Clock {
    /// The time of entity `e`: ticks it has been awake for since the world began (its `age`, with the world's start).
    #[inline]
    pub fn of(&self, e: Entity) -> u64 {
        self.0 - self.1.get(e.index().index() as usize).copied().unwrap_or(0)
    }
    /// The world's tick at which the entity's own time is `local`.
    #[inline]
    pub fn at(&self, e: Entity, local: u64) -> u64 {
        local + self.1.get(e.index().index() as usize).copied().unwrap_or(0)
    }
    /// The entity slept for `d` ticks of the world: its time stood still.
    pub(crate) fn sleep_for(&mut self, e: Entity, d: u64) {
        let i = e.index().index() as usize;
        if self.1.len() <= i {
            self.1.resize(i + 1, 0);
        }
        self.1[i] += d;
    }
    /// A new entity starts with its own time equal to the world's (its slot may have been an older entity's).
    fn start(&mut self, e: Entity) {
        if let Some(x) = self.1.get_mut(e.index().index() as usize) {
            *x = 0;
        }
    }
}
/// On every animal: its own time starts with its creation (see `Clock`).
#[derive(Component, Clone, Copy, Debug, Default)]
#[component(on_add = start_own_time)]
pub struct OwnTime;
fn start_own_time(mut world: bevy_ecs::world::DeferredWorld, ctx: bevy_ecs::lifecycle::HookContext) {
    world.resource_mut::<Clock>().start(ctx.entity);
}
/// Arms a wheel entry for an entity at its own time `local`.
pub fn arm_at(w: &mut World, e: Entity, local: u64, d: Due) {
    let at = w.resource::<Clock>().at(e, local);
    w.resource_mut::<Wheel>().push(at, e, d);
}
/// The functions `after` can call, each registered once.
#[derive(Resource, Default)]
pub struct Callbacks(Vec<fn(&mut World, Entity)>);
/// Calls `f(w, e)` once, `ticks` ticks of the entity's own time from now: a deadline on the wheel, no tick in between.
pub fn after(w: &mut World, e: Entity, ticks: i32, f: fn(&mut World, Entity)) {
    let cb = &mut w.get_resource_or_insert_with(Callbacks::default).0;
    let i = cb.iter().position(|&g| g as usize == f as usize).unwrap_or_else(|| {
        cb.push(f);
        cb.len() - 1
    });
    let at = now_of(w, e) + ticks.max(0) as u64;
    arm_at(w, e, at, Due::Call(i as u16));
}
/// The entity's own time.
pub fn now_of(w: &World, e: Entity) -> u64 {
    w.resource::<Clock>().of(e)
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Due {
    Stroll,
    Grow,
    Goal(u8),
    /// The hen's egg timer (`farm.rs`): a rule of the species, not a goal.
    Egg,
    /// A stuck projectile's life is over (`projectile.rs`).
    Expire,
    /// A sensor of the brain is due (its rank in the plan), and a memory's time is up (its slot and the token it had).
    Sensor(u8),
    Forget(u16, u32),
    /// A module's function runs for the entity (`after`): the index in `Callbacks`.
    Call(u16),
    /// The brain has something to do (a timed behaviour's end).
    BrainWake,
    /// A resting item's deadline (its last tick, its merge check), with the token it was armed with (`item.rs`).
    ItemExpire(u32),
    /// A merge check of an item at its own multiple of 40 ticks (`item.rs`).
    ItemCheck(u32),
    /// An orb's end of life and its merge check (`orb.rs`).
    /// A hopping body's wait is over (`locomotion/hop.rs`).
    Hop,
    OrbExpire(u32),
    OrbCheck(u32),
    Burn,
    /// A status effect acts or ends (`effects.rs`).
    Effect,
    /// A creeper's fuse may be at its end (`explosion.rs`).
    Fuse,
    /// The raids tick (`raids.rs`).
    Raids,
    /// `Mob.isSunBurnTick`'s draw came up (`hostile.rs`).
    Sun,
    /// The mob may go away from now on (`despawn.rs`).
    Despawn,
}
const WHEEL: usize = 4096;
/// A timer wheel: what is due at each tick, so that a sleeping or growing cow costs nothing until then.
#[derive(Resource)]
pub struct Wheel {
    slots: Vec<Vec<(Entity, Due)>>,
    /// What is due further than the wheel's horizon (`WHEEL` ticks): by tick, moved into the slots when it comes within the
    /// horizon. A deadline of ANY length can be pushed: no caller knows the width of the wheel.
    far: std::collections::BTreeMap<u64, Vec<(Entity, Due)>>,
    /// The tick the wheel is at (set by `advance`, each tick, before the slot is read).
    now: u64,
    /// With the simulation zone on (`zone.rs`): what each entity has in the wheel (by entity index), so that a creature
    /// that leaves the zone takes its entries out without a scan of the wheel.
    track: bool,
    owned: Vec<Vec<(Entity, u64, Due)>>,
}
impl Default for Wheel {
    fn default() -> Self {
        Self { slots: vec![Vec::new(); WHEEL], far: Default::default(), now: 0, track: false, owned: Vec::new() }
    }
}
impl Wheel {
    /// The wheel is at tick `now`: what has come within its horizon leaves the far list for its slot (called by `due`).
    pub(crate) fn advance(&mut self, now: u64) {
        self.now = now;
        while let Some(entry) = self.far.first_entry() {
            if *entry.key() >= now + WHEEL as u64 {
                break;
            }
            let (at, list) = entry.remove_entry();
            self.slots[(at % WHEEL as u64) as usize].extend(list);
        }
    }
    pub fn push(&mut self, at: u64, e: Entity, d: Due) {
        if at >= self.now + WHEEL as u64 {
            self.far.entry(at).or_default().push((e, d));
        } else {
            self.slots[(at % WHEEL as u64) as usize].push((e, d));
        }
        if self.track {
            self.own(e, at, d);
        }
    }
    fn own(&mut self, e: Entity, at: u64, d: Due) {
        let i = e.index().index() as usize;
        if self.owned.len() <= i {
            self.owned.resize_with(i + 1, Vec::new);
        }
        self.owned[i].push((e, at, d));
    }
    pub fn enable_tracking_pub(&mut self) {
        self.enable_tracking();
    }
    /// Starts to keep the entries of each entity (reads the ones already there).
    pub(crate) fn enable_tracking(&mut self) {
        self.track = true;
        self.owned.clear();
        for at in 0..WHEEL {
            for k in 0..self.slots[at].len() {
                let (e, d) = self.slots[at][k];
                // The slot of tick `at` holds the entries of that tick (an entry is at most WHEEL - 1 ticks ahead).
                self.own(e, at as u64, d);
            }
        }
    }
    /// An entry fired at tick `at`: it is no longer the entity's.
    fn fired(&mut self, e: Entity, at: u64, d: Due) {
        if let Some(list) = self.owned.get_mut(e.index().index() as usize)
            && let Some(i) = list.iter().position(|o| o.0 == e && o.2 == d && o.1 % WHEEL as u64 == at % WHEEL as u64)
        {
            list.swap_remove(i);
        }
    }
    /// Takes the entries of `e` out of the wheel: `(tick, what)`.
    pub(crate) fn take_entries(&mut self, e: Entity) -> Vec<(u64, Due)> {
        let Some(list) = self.owned.get_mut(e.index().index() as usize) else { return Vec::new() };
        let mine: Vec<(Entity, u64, Due)> = list.iter().copied().filter(|o| o.0 == e).collect();
        list.retain(|o| o.0 != e);
        let mut out = Vec::with_capacity(mine.len());
        for (_, at, d) in mine {
            let slot = if at >= self.now + WHEEL as u64 { self.far.entry(at).or_default() } else { &mut self.slots[(at % WHEEL as u64) as usize] };
            if let Some(i) = slot.iter().position(|o| o.0 == e && o.1 == d) {
                slot.swap_remove(i);
                out.push((at, d));
            }
        }
        out
    }
    /// How many entries `e` has in the wheel (tracking on).
    pub fn owned_by(&self, e: Entity) -> usize {
        self.owned.get(e.index().index() as usize).map_or(0, |l| l.iter().filter(|o| o.0 == e).count())
    }
    /// How many entries the wheel holds in all.
    pub fn len(&self) -> usize {
        self.slots.iter().map(Vec::len).sum::<usize>() + self.far.values().map(Vec::len).sum::<usize>()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// How many entries the slot of tick `at` holds.
    pub fn due_len(&self, at: u64) -> usize {
        self.slots[(at % WHEEL as u64) as usize].len()
    }
}
/// The width of an index column: a box (at most 1.4 blocks wide here) overlaps only entities within the 3 x 3 columns around it.
const CELL: f64 = 1.0;
/// The widening of a slot's extent (blocks): covers the `f32` rounding of the relative extent.
const SLOT_MARGIN: f64 = 1e-3;
/// The side of a block of columns of the index, in columns.
const BLOCK: usize = 8;
/// `BLOCK` as a signed column count.
const B: i64 = BLOCK as i64;
/// The spatial index: the entities by 2-block column of their box's centre, a resource. It is changed only when a
/// box changes column (`reindex`, after the move): a cow that does not move costs it nothing.
#[derive(Resource, Default)]
pub struct Index {
    pub(crate) origin: f64,
    pub(crate) width: usize,
    pub(crate) cells: Vec<Vec<Entity>>,
    /// Parallel to `cells`: the XZ extent of each entity's box (min x, max x, min z, max z) relative to the column's corner, widened by `SLOT_MARGIN`,
    /// as `f32` (16 bytes per entity): `push` skips the neighbours whose extent does not touch its own without an ECS lookup. An entity whose box
    /// `reindex` has not seen yet holds an infinite extent (always a candidate).
    slots: Vec<Vec<[f32; 4]>>,
    stamp: Vec<u32>,
    /// The entities whose cell changed since the last `player_near` (`reindex` writes them).
    pub(crate) crossed: Vec<(Entity, u32)>,
    /// The simulated chunks (`zone.rs`): off unless `Herd::set_simulation` turned it on.
    pub zone: crate::zone::Zone,
    /// How many animals each block of 8 x 8 columns holds (`player_near` looks only at the blocks that hold some).
    /// A bit per column of each block (row-major, 8 x 8): the columns that hold an animal.
    blocks: Vec<u64>,
    bw: usize,
    /// The watched columns of each consumer of `CellMoves` that looks at the NEIGHBOURS of a mover (see `Watch`).
    watch: [Watch; WATCHERS],
    /// By entity index: the consumers (bit `WATCH_*`) the entity is a watcher of.
    watching: Vec<u8>,
}
/// The consumers of the moves of columns that wake the watchers around a mover. A consumer registers its watchers (an entity
/// carrying its component, see `watch_hooks!`); a mover that enters a column far from every watcher of the consumer costs it nothing.
pub const WATCH_FOLLOW_PARENT: usize = 0;
pub const WATCH_AVOID: usize = 1;
pub const WATCH_FOLLOW_MOB: usize = 2;
pub const WATCH_PUFFER: usize = 3;
const WATCHERS: usize = 4;
/// The side of a bucket of watched columns: a watcher within 32 columns of a mover is in the 3 x 3 buckets around its column.
const BUCKET: i64 = 32;
/// The watchers of one consumer, counted by bucket of 32 x 32 columns; `total` is zero while nobody watches (the whole cost then).
#[derive(Default)]
struct Watch {
    total: u32,
    counts: Vec<u32>,
}
impl Index {
    pub fn new(origin: f64, width: usize) -> Self {
        Self { origin, width, cells: vec![Vec::new(); width * width], slots: vec![Vec::new(); width * width], stamp: vec![0; width * width], crossed: Vec::new(), zone: Default::default(), blocks: vec![0; width.div_ceil(BLOCK).pow(2)], bw: width.div_ceil(BLOCK), watch: Default::default(), watching: Vec::new() }
    }
    #[inline]
    fn bucket_of(&self, cell: u32) -> (usize, usize) {
        let (cx, cz) = ((cell as usize % self.width) as i64 / BUCKET, (cell as usize / self.width) as i64 / BUCKET);
        (cx as usize, cz as usize)
    }
    fn buckets(&self) -> usize {
        self.width.div_ceil(BUCKET as usize)
    }
    /// `e`, in column `cell`, watches for consumer `k` (adds it to the count of its bucket).
    fn watch_count(&mut self, k: usize, cell: u32, delta: i32) {
        let n = self.buckets();
        let (bx, bz) = self.bucket_of(cell);
        let w = &mut self.watch[k];
        if w.counts.is_empty() {
            w.counts = vec![0; n * n];
        }
        let c = &mut w.counts[bz * n + bx];
        *c = (*c as i64 + delta as i64) as u32;
        w.total = (w.total as i64 + delta as i64) as u32;
    }
    /// `e` became a watcher of consumer `k` (its component was inserted); `cell` is its column (`u32::MAX`: not placed yet,
    /// `move_watcher` counts it at its first column).
    pub fn watch_add(&mut self, e: Entity, k: usize, cell: u32) {
        let i = e.index().index() as usize;
        if self.watching.len() <= i {
            self.watching.resize(i + 1, 0);
        }
        self.watching[i] |= 1 << k;
        if cell != u32::MAX {
            self.watch_count(k, cell, 1);
        }
    }
    pub fn watch_remove(&mut self, e: Entity, k: usize, cell: u32) {
        if let Some(m) = self.watching.get_mut(e.index().index() as usize) {
            *m &= !(1 << k);
        }
        if cell != u32::MAX {
            self.watch_count(k, cell, -1);
        }
    }
    /// `reindex`: the entity went from column `old` to `new`; its counts follow when it watches and the bucket changed.
    #[inline]
    pub(crate) fn move_watcher(&mut self, e: Entity, old: u32, new: u32) {
        let m = self.watching.get(e.index().index() as usize).copied().unwrap_or(0);
        if m == 0 || (old != u32::MAX && self.bucket_of(old) == self.bucket_of(new)) {
            return;
        }
        for k in 0..WATCHERS {
            if m >> k & 1 == 1 {
                if old != u32::MAX {
                    self.watch_count(k, old, -1);
                }
                self.watch_count(k, new, 1);
            }
        }
    }
    /// Does consumer `k` have a watcher within 32 columns of column `cell`? (zero cost while it has none.)
    #[inline]
    pub fn watched_near(&self, k: usize, cell: u32) -> bool {
        let w = &self.watch[k];
        if w.total == 0 {
            return false;
        }
        let n = self.buckets();
        let (bx, bz) = self.bucket_of(cell);
        (bz.saturating_sub(1)..=(bz + 1).min(n - 1)).any(|z| (bx.saturating_sub(1)..=(bx + 1).min(n - 1)).any(|x| w.counts[z * n + x] > 0))
    }
    pub(crate) fn column(&self, x: f64) -> i64 {
        ((x - self.origin) / CELL).floor() as i64
    }
    pub fn cell_of(&self, x: f64, z: f64) -> u32 {
        let cx = self.column(x).clamp(0, self.width as i64 - 1) as usize;
        let cz = self.column(z).clamp(0, self.width as i64 - 1) as usize;
        (cz * self.width + cx) as u32
    }
    pub fn insert(&mut self, e: Entity, cell: u32) {
        self.cells[cell as usize].push(e);
        self.slots[cell as usize].push([f32::NEG_INFINITY, f32::INFINITY, f32::NEG_INFINITY, f32::INFINITY]);
        let (b, bit) = self.block_of(cell);
        self.blocks[b] |= bit;
    }
    #[inline]
    fn block_of(&self, cell: u32) -> (usize, u64) {
        let (cx, cz) = (cell as usize % self.width, cell as usize / self.width);
        ((cz / BLOCK) * self.bw + cx / BLOCK, 1 << ((cz % BLOCK) * BLOCK + cx % BLOCK))
    }
    pub fn remove(&mut self, e: Entity, cell: u32) {
        let list = &mut self.cells[cell as usize];
        if let Some(i) = list.iter().position(|&o| o == e) {
            list.swap_remove(i);
            self.slots[cell as usize].swap_remove(i);
            let emptied = list.is_empty();
            if emptied {
                let (b, bit) = self.block_of(cell);
                self.blocks[b] &= !bit;
            }
        }
    }
    /// Calls `f` for the entities in the columns within `r` blocks of `(x, z)` (a box is at most 2 blocks wide).
    /// Two levels: the blocks of 8 x 8 columns hold a bit per occupied column, so the cost follows the occupied columns, not the area.
    /// The visit order is the plain row-major order of the columns (rows outer, columns inner), as a loop over every column would give.
    pub fn for_radius(&self, x: f64, z: f64, r: f64, mut f: impl FnMut(Entity)) {
        let w = self.width as i64;
        let (x0, x1) = (self.column(x - r - 1.0).max(0), self.column(x + r + 1.0).min(w - 1));
        let (z0, z1) = (self.column(z - r - 1.0).max(0), self.column(z + r + 1.0).min(w - 1));
        if x0 > x1 || z0 > z1 {
            return;
        }
        let (bx0, bx1) = ((x0 / B) as usize, (x1 / B) as usize);
        let mut zz = z0;
        while zz <= z1 {
            let band = (zz / B) as usize * self.bw;
            let row_end = z1.min(zz / B * B + B - 1);
            if self.blocks[band + bx0..=band + bx1].iter().all(|&m| m == 0) {
                zz = row_end + 1;
                continue;
            }
            for z in zz..=row_end {
                let shift = (z % B) * B;
                for bx in bx0..=bx1 {
                    let mut bits = (self.blocks[band + bx] >> shift) & 0xff;
                    if bits == 0 {
                        continue;
                    }
                    let lo = if bx == bx0 { x0 % B } else { 0 };
                    let hi = if bx == bx1 { x1 % B } else { B - 1 };
                    bits &= (0xffu64 << lo) & (0xffu64 >> (B - 1 - hi));
                    while bits != 0 {
                        let i = bits.trailing_zeros() as i64;
                        bits &= bits - 1;
                        for &e in &self.cells[(z * w + bx as i64 * B + i) as usize] {
                            f(e);
                        }
                    }
                }
            }
            zz = row_end + 1;
        }
    }
    /// Calls `f` for the entities whose column may lie in the disk of radius `r` around `old` and not in the one around `new` (a
    /// superset: the crescent a player's step sweeps, or a whole disk when `new` is `None`), row by row: the cost follows the length
    /// of the crescent, not the area of the disk.
    pub fn crescent(&self, old: (f64, f64), new: Option<(f64, f64)>, r: f64, mut f: impl FnMut(Entity)) {
        let w = self.width as i64;
        let (r, ro) = (r + 1.0, r - 1.0);
        for zz in self.column(old.1 - r).max(0)..=self.column(old.1 + r).min(w - 1) {
            let (z0, z1) = (self.origin + zz as f64 * CELL, self.origin + (zz + 1) as f64 * CELL);
            let near = (old.1.clamp(z0, z1) - old.1).abs();
            if near >= r {
                continue;
            }
            let half = (r * r - near * near).sqrt();
            let (mut a, b) = (self.column(old.0 - half).max(0), self.column(old.0 + half).min(w - 1));
            // The columns surely inside the new disk (its chord at the far edge of the row) are left out: two runs.
            let skip = new.and_then(|n| {
                let far = (n.1 - z0).abs().max((n.1 - z1).abs());
                (far < ro).then(|| ((ro * ro - far * far).sqrt(), n.0))
            });
            let mut run = |lo: i64, hi: i64, f: &mut dyn FnMut(Entity)| {
                for xx in lo..=hi {
                    for &e in &self.cells[(zz * w + xx) as usize] {
                        f(e);
                    }
                }
            };
            if let Some((h, nx)) = skip {
                let (s0, s1) = (self.column(nx - h + 1.0), self.column(nx + h - 1.0));
                run(a, b.min(s0 - 1), &mut f);
                a = a.max(s1 + 1);
            }
            run(a, b, &mut f);
        }
    }
    /// Calls `f` for the entities in the columns around `(x, z)` (3 x 3, a box is at most 2 blocks wide).
    /// Records the XZ extent of the box of `e`, which is in `cell` (see `slots`).
    pub(crate) fn set_box(&mut self, e: Entity, cell: u32, mn: (f64, f64), mx: (f64, f64)) {
        let Some(i) = self.cells[cell as usize].iter().rposition(|&o| o == e) else { return };
        let (x0, z0) = self.corner(cell);
        self.slots[cell as usize][i] = [(mn.0 - x0 - SLOT_MARGIN) as f32, (mx.0 - x0 + SLOT_MARGIN) as f32, (mn.1 - z0 - SLOT_MARGIN) as f32, (mx.1 - z0 + SLOT_MARGIN) as f32];
    }
    #[inline]
    fn corner(&self, cell: u32) -> (f64, f64) {
        let (cx, cz) = (cell as usize % self.width, cell as usize / self.width);
        (self.origin + cx as f64 * CELL, self.origin + cz as f64 * CELL)
    }
    /// `for_near` for the entities whose box may touch the XZ extent `mn..mx` (a superset: the caller tests the real boxes).
    pub fn for_near_touching(&self, x: f64, z: f64, mn: (f64, f64), mx: (f64, f64), mut f: impl FnMut(Entity)) {
        let (cx, cz) = (self.column(x), self.column(z));
        let w = self.width as i64;
        for zz in (cz - 1).max(0)..=(cz + 1).min(w - 1) {
            for xx in (cx - 1).max(0)..=(cx + 1).min(w - 1) {
                let c = (zz * w + xx) as usize;
                let list = &self.cells[c];
                if list.is_empty() {
                    continue;
                }
                let (x0, z0) = self.corner(c as u32);
                let (ax, bx, az, bz) = ((mn.0 - x0) as f32, (mx.0 - x0) as f32, (mn.1 - z0) as f32, (mx.1 - z0) as f32);
                for (&e, s) in list.iter().zip(&self.slots[c]) {
                    if s[1] >= ax && s[0] <= bx && s[3] >= az && s[2] <= bz {
                        f(e);
                    }
                }
            }
        }
    }
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
    /// The occupancy masks of the world's sections (`occupancy.rs`): the block kernel's fast path.
    pub occ: &'static crate::occupancy::Occupancy,
    /// The walls are at |x| or |z| = `half`.
    pub half: i32,
}
impl Level {
    /// `MobWorld::no_collision` through the occupancy masks.
    pub fn no_collision(&self, b: Aabb) -> bool {
        let mw = self.mw.0;
        self.occ.no_collision(SceneCollisions { world: mw.world, binding: mw.collision, subject: None }, b)
    }
}
#[derive(Resource, Clone, Copy)]
pub struct Config {
    pub sleep: bool,
    pub parallel: bool,
    /// The pushes of the animals that overlap another run in the `Mob`'s order (`ordered_pass`): see `PushMode`.
    pub ordered_push: bool,
    /// Batch mode: an animal that overlaps another stays in `push` every tick even if it does not move (`vanilla` runs
    /// `pushEntities` for every entity, at rest or not).
    pub keep_touching: bool,
    /// The most path searches `pathfind` runs in a tick (0: no limit). The others keep `NeedsPath` and are served the next tick, in the table order
    /// (first asked, first served, in the main); a guard against the bursts (suivi/ecs-echelle.md, H): a zombie that waits does not move.
    pub path_budget: usize,
    /// EXPERIMENT (calibration of the batch push): the form (0: all impulses x k; 1: only the impulse from a lower index x k)
    /// and the factor k, from the environment (PUSH_FORM, PUSH_K).
    pub push_form: u8,
    pub push_k: f64,
}
/// `getWalkTargetValue` of the template `Mob`, as data (the species the prototype ports; the others read zero).
fn walk_value_of(mob: &Mob, kind: &horizon_client_lib::mobs::MobType) -> crate::spots::WalkValue {
    use crate::spots::WalkValue as W;
    use horizon_client_lib::mobs::Species as S;
    match mob.species() {
        S::Mooshroom => W::Mooshroom,
        S::Bee => W::Bee,
        S::Turtle => W::Turtle,
        S::Enderman | S::Pillager | S::Warden | S::Axolotl | S::Nautilus | S::ZombieNautilus | S::Creaking => W::Zero,
        S::Slime | S::MagmaCube | S::Ghast | S::Phantom => W::Zero,
        _ if mob.hostile() => W::Monster,
        _ if kind.flags.animal => W::Animal,
        _ => W::Zero,
    }
}
/// The species' navigation data, read once from a real `Mob` of the type.
/// The species of an entity: an index into the `SpeciesTable`.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
#[component(on_add = census_added, on_remove = census_removed)]
pub struct SpeciesId(pub u8);
/// How many entities of each species are alive (by `SpeciesId`): kept by the hooks, read by the goals that scan for a species.
/// It lives in `Carriers` (a resource of its own would be one more entity, and shift the bench's picks).
#[derive(Default)]
pub struct Census(pub Vec<u32>);
fn census_added(mut w: bevy_ecs::world::DeferredWorld, ctx: bevy_ecs::lifecycle::HookContext) {
    let id = w.get::<SpeciesId>(ctx.entity).unwrap().0 as usize;
    let first = w.get_resource_mut::<crate::goals::Carriers>().is_some_and(|mut c| {
        let c = &mut c.1;
        if c.0.len() <= id {
            c.0.resize(id + 1, 0);
        }
        c.0[id] += 1;
        c.0[id] == 1
    });
    if first {
        w.commands().queue(move |w: &mut World| crate::behaviours::nearest_target::unpark(w, id as u8));
    }
}
fn census_removed(mut w: bevy_ecs::world::DeferredWorld, ctx: bevy_ecs::lifecycle::HookContext) {
    let id = w.get::<SpeciesId>(ctx.entity).unwrap().0 as usize;
    if let Some(mut c) = w.get_resource_mut::<crate::goals::Carriers>() {
        let c = &mut c.1;
        if let Some(n) = c.0.get_mut(id) {
            *n = n.saturating_sub(1);
        }
    }
}
/// A set of species by `SpeciesId`: 256 bits, one per value of the id (no species is left out of a set).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SpeciesSet([u64; 4]);
impl SpeciesSet {
    pub const EMPTY: Self = Self([0; 4]);
    pub fn with(mut self, id: usize) -> Self {
        self.0[id >> 6 & 3] |= 1 << (id & 63);
        self
    }
    pub fn has(&self, id: u8) -> bool {
        self.0[usize::from(id) >> 6] >> (id & 63) & 1 == 1
    }
    pub fn is_empty(&self) -> bool {
        self.0 == [0; 4]
    }
}
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
    pub lava_walker: bool,
    /// `EntityTypeTags.UNDEAD` (the type's flag): poison and regeneration do nothing to it, healing hurts and harming heals.
    pub undead: bool,
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
    /// `Attributes.FLYING_SPEED` and the `air_speed` of the type (`LivingEntity.getFlyingSpeed`).
    pub flying_speed: f64,
    pub air_speed: f32,
    /// `EntityType` flag: the vertical drag is the horizontal one (0.91) rather than 0.98 (a flier of the air).
    pub omnidirectional: bool,
    pub baby: (f32, f32),
    pub goals: crate::species::Config,
    /// The farm behaviours of the species (`farm.rs`).
    pub rules: &'static [crate::rules::RuleEntry],
    /// `Attributes.KNOCKBACK_RESISTANCE`.
    pub knockback_resistance: f64,
    /// The base armor and toughness of the type (`Attributes.ARMOR`, `ARMOR_TOUGHNESS`): worn pieces add to it.
    pub armor: (f32, f32),
    /// `Attributes.ATTACK_DAMAGE` (0 for a species that does not attack).
    pub attack_damage: f32,
    /// `EntityType` flag: the undead that burn in daylight.
    /// What the species prefers to walk to (`getWalkTargetValue`).
    pub walk: crate::spots::WalkValue,
    /// The capabilities of the configuration (its sections): the entity is made of these only.
    pub caps: &'static [crate::behaviours::Cap],
    /// The brain of the species, resolved (None: it has none).
    pub brain: Option<&'static crate::brain::Plan>,
    /// The modes of locomotion (`locomotion.rs`).
    pub modes: &'static [crate::locomotion::ModeEntry],
}
/// The last attacker (`LivingEntity.getLastHurtByMob`): written by `apply_damage` for every hit that has an attacker; `at` is
/// the entity's own time (a module forgets it after 100 ticks as the `Mob` does).
#[derive(Component, Clone, Copy, Debug)]
pub struct LastHurtBy {
    pub who: crate::targeting::Who,
    pub at: u64,
}
/// What the fluid physics reads of an entity (a capability of the body: an object without it uses `DEFAULT`).
#[derive(Component, Clone, Copy, Debug)]
pub struct FluidParams {
    pub eye_height: f64,
    pub can_float: bool,
    pub water_slowdown: f32,
    pub water_efficiency: f32,
}
impl FluidParams {
    pub const DEFAULT: FluidParams = FluidParams { eye_height: 0.0, can_float: false, water_slowdown: 0.8, water_efficiency: 0.0 };
}
/// What the path search reads of an entity that walks.
#[derive(Component, Clone, Copy)]
pub struct NavParams {
    pub malus: [Option<f32>; PATH_TYPES],
    pub can_float: bool,
    pub can_open_doors: bool,
    pub can_pass_doors: bool,
    pub can_walk_over_fences: bool,
    pub follow_range: f32,
}
/// `Attributes.ATTACK_DAMAGE` (0: the entity does not attack).
#[derive(Component, Clone, Copy, Debug)]
pub struct AttackDamage(pub f32);
/// `Attributes.KNOCKBACK_RESISTANCE` (a living entity).
#[derive(Component, Clone, Copy, Debug)]
pub struct Resist(pub f64);
/// What the entity does not suffer, as bits (a living entity).
#[derive(Component, Clone, Copy, Debug)]
pub struct Immune(pub u32);
/// No fall damage.
pub const IMMUNE_FALL: u32 = 1;
/// Does not drown under water.
pub const BREATHES_WATER: u32 = 2;
/// No window of invulnerability after a hurt (`ItemEntity.hurtServer` takes each hit in full; a living entity ignores the weaker ones for 10 ticks).
pub const NO_HURT_COOLDOWN: u32 = 4;
impl Immune {
    pub fn has(self, bit: u32) -> bool {
        self.0 & bit != 0
    }
}
/// The type of the entity (`minecraft:cow`...), for the data keyed by type of the host (breathing).
#[derive(Component, Clone, Copy, Debug)]
pub struct TypeName(pub &'static str);
#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
pub struct CowTick;

/// A walled stone slab of `half` blocks each way from the origin, on the prepared vanilla data.
pub fn build_level(half: i32, patches: bool) -> (Level, Species, Arc<horizon_client_lib::mobs::MobType>) {
    build_level_with(half, move |blocks: &mut horizon_client_render::world::World, registry: &horizon_world::registry::BlockRegistry| {
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
    })
}
/// `build_level` over a world of the caller's making (`build` fills the blocks).
pub fn build_level_with(half: i32, build: impl FnOnce(&mut horizon_client_render::world::World, &horizon_world::registry::BlockRegistry)) -> (Level, Species, Arc<horizon_client_lib::mobs::MobType>) {
    let (mw, types, blocks_ptr) =
        horizon_client_lib::mobs::world::leaked_test_world_mut(true, build).expect("HORIZON_TEST_RESOURCES");
    let chosen: &'static str = Box::leak(
        std::env::var("HORIZON_SPECIES").unwrap_or_else(|_| crate::species::PROCESS_DEFAULT.into()).into_boxed_str(),
    );
    // Every species the prototype knows, by id: the same table for the whole process.
    let names: Vec<&'static str> = crate::species::names();
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
            lava_walker: kind.flags.lava_walker,
            undead: kind.flags.undead,
            water_efficiency: kind.water_efficiency as f32,
            eye_height: kind.eye_height,
            breathes_water: kind.flags.breathes_water,
            name,
            width: kind.width as f32,
            height: kind.height as f32,
            step_height: kind.step_height as f32,
            gravity: kind.gravity,
            speed: mob.attrs.speed,
            flying_speed: horizon_client_lib::mobs::finalize::value(&mob, horizon_client_lib::mobs::finalize::Attr::FlyingSpeed),
            air_speed: kind.air_speed,
            omnidirectional: kind.flags.omnidirectional,
            baby: ((kind.width * kind.baby_scale) as f32, (kind.height * kind.baby_scale) as f32),
            goals: crate::species::config(name),
            rules: crate::species::registry().by_name(name).unwrap().rules,
            knockback_resistance: mob.attrs.knockback_resistance,
            armor: (kind.armor as f32, kind.armor_toughness as f32),
            attack_damage: mob.attrs.attack_damage,
            walk: walk_value_of(&mob, &kind),
            caps: crate::species::registry().by_name(name).unwrap().caps,
            brain: crate::species::registry().by_name(name).unwrap().brain,
            modes: crate::species::registry().by_name(name).unwrap().modes,
        });
        if name == chosen {
            chosen_kind = Some(kind);
        }
    }
    if let Some(items) = mw.items {
        crate::items::install(items);
        crate::equipment_items::install(items);
    }
    let _ = TYPES.set(types);
    let _ = TABLE.set(table.clone());
    let species = *table.iter().find(|s| s.name == chosen).expect("a known species");
    let _ = SPECIES.set(species);
    (Level { mw: Mw(mw), blocks: Blocks(blocks_ptr), occ: Box::leak(Box::default()), half }, species, chosen_kind.expect("a known species"))
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

/// The name of the species of the process (`HORIZON_SPECIES`), readable before any level is built (`species()` is not).
pub fn process_species_name() -> String {
    std::env::var("HORIZON_SPECIES").unwrap_or_else(|_| crate::species::PROCESS_DEFAULT.into())
}
/// For the tests of a behaviour only some species have: whether to skip the test of the process's species, which has no
/// such goal (the skip is printed, so it is never silent).
pub fn skip_unless(module: &'static crate::behaviours::Module, test: &str) -> bool {
    // By the name the process was asked for (`HORIZON_SPECIES`), before any level is built.
    let name = std::env::var("HORIZON_SPECIES").unwrap_or_else(|_| crate::species::PROCESS_DEFAULT.into());
    let has = crate::species::config(&name).has(module);
    if !has {
        eprintln!("SKIP {test}: {name} has no {} goal", module.name);
    }
    !has
}
/// For the tests written for a land animal that walks, strolls and has goals: they declare what they need (the capabilities
/// `Locomotion`, `Goals` and `Head`, a goal list, a walk and no other medium) and skip, printed, in a process whose species lacks
/// it: the rule of the code (nothing knows a species) applies to the tests.
pub fn skip_unless_walker(test: &str) -> bool {
    skip_unless_caps(&[crate::behaviours::Cap::Locomotion, crate::behaviours::Cap::Goals, crate::behaviours::Cap::Head], test)
        || skip_unless_ground_only(test)
        || skip_unless_goals(test)
}
/// For the tests of the farm animal's goals (tempt, breed, follow the parent, panic, stroll): skips (printed) unless the species
/// of the process has them all.
pub fn skip_unless_farm_goals(test: &str) -> bool {
    use crate::behaviours::{breed, follow_parent, panic, stroll, tempt};
    let name = std::env::var("HORIZON_SPECIES").unwrap_or_else(|_| crate::species::PROCESS_DEFAULT.into());
    let cfg = crate::species::registry().by_name(&name).map(|r| r.config);
    let ok = cfg.is_ok_and(|c| c.has(&tempt::MODULE) && c.has(&breed::MODULE) && c.has(&follow_parent::MODULE) && c.has(&panic::MODULE) && c.has(&stroll::MODULE));
    if !ok {
        eprintln!("SKIP {test}: {name} lacks the farm animal's goals (tempt, breed, follow parent, panic, stroll)");
    }
    !ok
}
/// For the physics scenes written for a small animal (slabs, herds spaced for a cow's box, a pit of 24): skips (printed) when the
/// species of the process has a body of another scale (wider than a block, taller than a block and a half, or smaller than half a block).
pub fn skip_unless_small_body(test: &str) -> bool {
    let name = std::env::var("HORIZON_SPECIES").unwrap_or_else(|_| crate::species::PROCESS_DEFAULT.into());
    let (w, h) = species_size(&name);
    let skip = w > 0.95 || h > 1.5 || (w < 0.5 && h <= 0.5);
    if skip {
        eprintln!("SKIP {test}: {name} is {w} x {h}: the scene is made for a small animal");
    }
    skip
}
fn species_size(name: &str) -> (f32, f32) {
    // The size of the type, read once from the real data of the process.
    static TYPES_FOR_SIZE: std::sync::OnceLock<Option<horizon_client_lib::mobs::MobTypes>> = std::sync::OnceLock::new();
    let t = TYPES_FOR_SIZE.get_or_init(|| horizon_client_lib::mobs::MobTypes::load(std::path::Path::new(&std::env::var("HORIZON_TEST_RESOURCES").unwrap_or_default())).ok());
    t.as_ref().and_then(|t| t.types.get(name).map(|k| (k.width as f32, k.height as f32))).unwrap_or((0.9, 1.4))
}
/// Skips (printed) for a species that the fire and the lava do not hurt (the strider walks on lava): the framework has no fire immunity yet
/// (NEED: an `Immune` bit for fire and lava, then a hazard module that reads it), so the ECS strider burns where the `Mob`'s does not.
pub fn skip_if_fire_immune(test: &str) -> bool {
    let name = std::env::var("HORIZON_SPECIES").unwrap_or_else(|_| crate::species::PROCESS_DEFAULT.into());
    let immune = crate::species::registry().by_name(&name).is_ok_and(|r| r.modes.iter().any(|m| m.0 == crate::locomotion::Medium::Lava));
    if immune {
        eprintln!("SKIP {test}: {name} walks on lava: the framework has no fire immunity yet");
    }
    immune
}
/// `HORIZON_QUICK` (set by `suites.sh`, `FULL=1` removes it): the scenes that repeat a trial many times (laws against the
/// `Mob` over dozens of seeds) do a quarter of them. 
pub fn trials(n: usize) -> usize {
    if std::env::var_os("HORIZON_QUICK").is_some() { (n / 4).max(3) } else { n }
}
/// Skips (printed) in the process of any species but the default one: a law test that builds its OWN configuration (the enderman's
/// block, the panda's roll, the horse's rear...) gives the same answer whatever species the process has, and costs seconds: it runs once.
pub fn skip_unless_default_species(test: &str) -> bool {
    let name = std::env::var("HORIZON_SPECIES").unwrap_or_else(|_| crate::species::PROCESS_DEFAULT.into());
    let skip = name != crate::species::PROCESS_DEFAULT;
    if skip {
        eprintln!("SKIP {test}: a law test of its own configuration, run once (in the process of {})", crate::species::PROCESS_DEFAULT);
    }
    skip
}
/// Skips (printed) unless the species of the process has all these capabilities.
pub fn skip_unless_caps(caps: &[crate::behaviours::Cap], test: &str) -> bool {
    let name = std::env::var("HORIZON_SPECIES").unwrap_or_else(|_| crate::species::PROCESS_DEFAULT.into());
    let have = crate::species::registry().by_name(&name).map(|r| r.caps).unwrap_or(&[]);
    let missing: Vec<_> = caps.iter().filter(|c| !have.contains(c)).collect();
    if !missing.is_empty() {
        eprintln!("SKIP {test}: {name} lacks the capabilities {missing:?}");
    }
    !missing.is_empty()
}
/// Skips (printed) when the species of the process moves in another medium than the ground (a swimmer).
pub fn skip_unless_ground_only(test: &str) -> bool {
    let name = std::env::var("HORIZON_SPECIES").unwrap_or_else(|_| crate::species::PROCESS_DEFAULT.into());
    // A ground mode other than the walk (the bounds of a slime or a rabbit) is another movement: the walking scenes do not apply.
    let other = crate::species::registry().by_name(&name).is_ok_and(|r| {
        r.modes.iter().any(|m| m.0 != crate::locomotion::Medium::Ground || !std::ptr::eq(m.1.module, &crate::locomotion::walk::MODULE))
    });
    if other {
        eprintln!("SKIP {test}: {name} moves in another medium than the ground, or other than by walking");
    }
    other
}
/// Skips (printed) when the species of the process has no goal at all (an entity driven by something else).
pub fn skip_unless_goals(test: &str) -> bool {
    let name = std::env::var("HORIZON_SPECIES").unwrap_or_else(|_| crate::species::PROCESS_DEFAULT.into());
    let none = crate::species::registry().by_name(&name).is_ok_and(|r| r.config.entries.is_empty());
    if none {
        eprintln!("SKIP {test}: {name} has no goal");
    }
    none
}
/// The reverse: a test of a passive behaviour (a player in range changes nothing) skips a species that has the goal
/// (a zombie hunts the player the test puts in range), printed.
pub fn skip_if(module: &'static crate::behaviours::Module, test: &str) -> bool {
    let name = std::env::var("HORIZON_SPECIES").unwrap_or_else(|_| crate::species::PROCESS_DEFAULT.into());
    let has = crate::species::config(&name).has(module);
    if has {
        eprintln!("SKIP {test}: {name} has a {} goal: the player in range makes it hunt, the test measures passive looking", module.name);
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
/// The capabilities of an entity are separate bundles: a configuration without a section has none of its components.
/// `Transform`: where it is, how fast, its own time and its identity. All a projectile has.
pub fn transform_bundle(sp: &Species, x: f64, z: f64, uid: u64) -> impl Bundle {
    (Pos(Vec3::new(x, 1.0, z)), Vel(Vec3::ZERO), SpeciesId(sp.id), crate::goals::Uid(uid), OwnTime)
}
/// `Solid`: a box in the spatial index (what the pushes, the searches and the targeting see).
pub fn solid_bundle(sp: &Species, x: f64, z: f64) -> impl Bundle {
    (Bounds(box_at(Vec3::new(x, 1.0, z), sp.width, sp.height)), Cell(u32::MAX))
}
/// `Body`: the physics of a body that falls, lands, floats and is pushed by the water and the blocks (`travel`).
pub fn body_bundle(sp: &Species) -> impl Bundle {
    (
        Body { width: sp.width, height: sp.height, step_height: sp.step_height, gravity: sp.gravity, speed: sp.speed, fall_factor: crate::rules::attrs_of(sp.rules).fall_factor.unwrap_or(1.0), object: false, orb: false, lava_walker: sp.lava_walker },
        Contact { on_ground: false, horizontal: false, fall_distance: 0.0 },
        FluidParams { eye_height: sp.eye_height, can_float: sp.can_float, water_slowdown: sp.water_slowdown, water_efficiency: sp.water_efficiency },
        crate::targeting::Eye(sp.eye_height),
        FluidState::default(),
        Recheck,
        Moving,
    )
}
/// `Living`: health, damage, growth, air, cold, what it resists and is immune to.
pub fn living_bundle(sp: &Species) -> impl Bundle {
    (
        Sizes { adult: (sp.width, sp.height), baby: sp.baby },
        Health(sp.max_health),
        HurtAt { at: 0, last: 0.0 },
        Frozen(0),
        Air(300),
        Resist(sp.knockback_resistance),
        Immune(crate::rules::attrs_of(sp.rules).immune | if sp.breathes_water { BREATHES_WATER } else { 0 }),
        TypeName(sp.name),
    )
}
/// `Locomotion`: walking and paths (the heading, the walking input, the path and its target, the walking data).
pub fn locomotion_bundle(sp: &Species, yaw: f32) -> impl Bundle {
    (
        Yaw(yaw),
        crate::locomotion::Modes(sp.modes),
        Drive::default(),
        MoveTo::default(),
        Nav::default(),
        Stroll { walking: false, scheduled: false, tx: 0.0, tz: 0.0, ty: 1, horizontal_range: 10, vertical_range: 7, interval: 60, plain: 0.001 },
        sp.walk,
        NavParams {
            malus: sp.malus,
            can_float: sp.can_float,
            can_open_doors: sp.can_open_doors,
            can_pass_doors: sp.can_pass_doors,
            can_walk_over_fences: sp.can_walk_over_fences,
            follow_range: sp.follow_range,
        },
    )
}
/// `Targets`: what the target selectors and the attacks read.
pub fn targets_bundle(sp: &Species) -> impl Bundle {
    (crate::targeting::FollowRange(f64::from(sp.follow_range)), AttackDamage(sp.attack_damage))
}
/// `Goals`: the selector, its head, and the activity zone and random stream the goals use.
fn goals_part(sp: &Species, uid: u64, rng: Rng) -> impl Bundle {
    (ActiveUntil(100), rng, crate::goals::goals_bundle(sp, uid))
}
/// An entity with EVERY capability at (x, z), for the tests and the benches: the species' data are the numbers.
/// `spawn_species` gives only the capabilities a configuration declares.
pub fn spawn_bundle(sp: &Species, x: f64, z: f64, seed: u64) -> impl Bundle {
    let mut rng = Rng(seed | 1);
    let yaw = (rng.next_u32() % 360) as f32;
    let uid = crate::goals::next_uid();
    (transform_bundle(sp, x, z, uid), solid_bundle(sp, x, z), body_bundle(sp), living_bundle(sp), locomotion_bundle(sp, yaw), targets_bundle(sp), crate::goals::head_bundle(sp), goals_part(sp, uid, rng))
}
/// Creates the entity with the capabilities `sp` declares and no others (body first: the goals' `equip` reads it).
fn spawn_declared(world: &mut World, sp: &Species, x: f64, z: f64, seed: u64) -> Entity {
    let mut rng = Rng(seed | 1);
    let yaw = (rng.next_u32() % 360) as f32;
    let uid = crate::goals::next_uid();
    let has = |c: crate::behaviours::Cap| sp.caps.contains(&c);
    let mut e = world.spawn(transform_bundle(sp, x, z, uid));
    if has(crate::behaviours::Cap::Solid) {
        e.insert(solid_bundle(sp, x, z));
    }
    if has(crate::behaviours::Cap::Body) {
        e.insert(body_bundle(sp));
    }
    if has(crate::behaviours::Cap::Living) {
        e.insert(living_bundle(sp));
        if sp.undead {
            e.insert(crate::effects::Undead);
        }
    }
    if has(crate::behaviours::Cap::Locomotion) {
        e.insert(locomotion_bundle(sp, yaw));
    }
    if has(crate::behaviours::Cap::Targets) {
        e.insert(targets_bundle(sp));
    }
    if has(crate::behaviours::Cap::Head) {
        e.insert(crate::goals::head_bundle(sp));
    }
    if has(crate::behaviours::Cap::Goals) {
        e.insert(goals_part(sp, uid, rng));
    }
    if has(crate::behaviours::Cap::Rideable) {
        e.insert(crate::mount::Mount::default());
    }
    if has(crate::behaviours::Cap::Tamable) {
        e.insert(crate::tame::Owner::default());
    }
    if has(crate::behaviours::Cap::Brain)
        && let Some(plan) = sp.brain
    {
        e.insert(crate::brain::bundle(plan));
    }
    e.id()
}
/// A new animal is in the spatial index from its creation (not after its first tick: the pushes and the searches of that
/// tick see it).
pub fn enter_index(world: &mut World, e: Entity) {
    let Some(b) = world.get::<Bounds>(e).copied() else { return };
    let (mn, mx) = (b.0.min_position(), b.0.max_position());
    let cell = world.resource::<Index>().cell_of((mn.x + mx.x) / 2.0, (mn.z + mx.z) / 2.0);
    let old = world.get::<Cell>(e).unwrap().0;
    {
        let mut idx = world.resource_mut::<Index>();
        if old != u32::MAX {
            idx.remove(e, old);
        }
        idx.insert(e, cell);
        idx.move_watcher(e, old, cell);
    }
    world.get_mut::<Cell>(e).unwrap().0 = cell;
    world.resource_mut::<crate::goals::CellMoves>().0.push(e);
}
/// A new entity of the species of `parent` (a baby), in the index; the framework looks the species up, the module does not.
pub fn spawn_like(world: &mut World, parent: Entity, x: f64, z: f64, seed: u64) -> Entity {
    let sp = world.resource::<SpeciesTable>().0[world.get::<SpeciesId>(parent).unwrap().0 as usize];
    let e = spawn_declared(world, &sp, x, z, seed);
    enter_index(world, e);
    crate::rules::equip_all(world, e, sp.rules);
    e
}
/// `spawn_species` and the first deadlines of its modules (what a creature born at the start of a scene needs; `breed` does it for
/// a baby). `spawn_species` alone arms nothing: the herds arm in bulk (`goals::arm_all`), a test calls this or `arm_all`.
pub fn spawn_armed(world: &mut World, sp: &Species, x: f64, z: f64, seed: u64) -> Entity {
    let e = spawn_species(world, sp, x, z, seed);
    crate::goals::arm_timers(world, e);
    e
}
/// Spawns an animal of species `sp` (with the extra data of a species that attacks).
pub fn spawn_species(world: &mut World, sp: &Species, x: f64, z: f64, seed: u64) -> Entity {
    let e = spawn_declared(world, sp, x, z, seed);
    enter_index(world, e);
    crate::rules::equip_all(world, e, sp.rules);
    crate::brain::start(world, e);
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
    s.add_systems((crate::zone::zone_step.run_if(crate::zone::zone_on), due, apply_damage, block_changes, recompute, player_near, pathfind, follow, path_ended, move_control, fluid_update, breathing, wall_check, travel, ordered_pass, reindex, hazard_tick, wall_damage, push, apply_pushes).chain());
    s.add_systems(cramming.after(apply_pushes));
    crate::goals::register(&mut s);
    s.add_systems(crate::despawn::step.after(crate::goals::online_events).before(player_near));
    s.add_systems(crate::spawning::step.after(crate::goals::online_events).before(player_near));
    crate::farm::register(&mut s);
    crate::projectile::register(&mut s);
    crate::brain::register(&mut s);
    crate::item::register(&mut s);
    crate::orb::register(&mut s);
    crate::poi::register(&mut s);
    crate::tame::register(&mut s);
    crate::mount::register(&mut s);
    crate::hostile::register(&mut s);
    s
}

// ---- systems ---------------------------------------------------------------------------------------------
/// `RandomStrollGoal.canUse` and `LandRandomPos`: after the wait, 10 tries of a spot within 10 x 7 (the first valid
/// one wins as all the ground weighs the same); the path to it is made by `pathfind`.
/// Arms the stroll attempt `idle_wait` ticks from now (the goal selector asks every other tick, 1 chance in 60).
pub(crate) fn arm_stroll(wheel: &mut Wheel, clock: &Clock, e: Entity, s: &mut Stroll, r: &mut Rng) {
    let now = clock.of(e);
    let at = now + idle_wait(r, s.interval).max(1) as u64;
    wheel.push(clock.at(e, at), e, Due::Stroll);
    s.scheduled = true;
}
/// `RandomStrollGoal.canUse` and `LandRandomPos`, at the tick the attempt was due: 10 tries of a spot within
/// 10 x 7 (the first valid one wins as all the ground weighs the same); the path to it is made by `pathfind`.
/// Returns whether a spot was picked.
pub(crate) fn pick_spot(level: &Level, pos: &Pos, s: &mut Stroll, r: &mut Rng, nav: &NavParams, walk: crate::spots::WalkValue, in_water: bool) -> bool {
    use crate::spots::{spotter, Pick};
    let (h, v) = (s.horizontal_range, s.vertical_range);
    let mut sp = spotter(level, [pos.0.x, pos.0.y, pos.0.z], nav, walk, r);
    // `WaterAvoidingRandomStrollGoal.getPosition`: in water a land spot first, else a land spot, a plain one 1 time in 1000.
    let picked = if in_water {
        sp.pick(Pick::Land { h: h + 5, v }).or_else(|| sp.pick(Pick::Default { h, v }))
    } else if sp.rng.next_float() >= s.plain {
        sp.pick(Pick::Land { h, v })
    } else {
        sp.pick(Pick::Default { h, v })
    };
    let Some(p) = picked else { return false };
    s.walking = true;
    s.tx = p[0];
    s.tz = p[2];
    s.ty = p[1].floor() as i32;
    true
}

/// The cells of row `zz` (columns, inclusive; a run is empty when its start is past its end) that may lie in one of
/// two circles of 32 blocks (a player's place before and after, `None`: absent) and not in the other: the cells either
/// circle touches except those both surely contain, as two runs. A superset, with a margin of a cell or two.

/// `Mob.checkDespawn`'s reset of `noActionTime`, by event: nothing is read while the players stay where they are
/// and the cows stay in their cells. When a player moves (`Players.dirty`), only the cows of the cells its old and
/// new circles of 32 blocks do not both contain or both exclude are tested (the thin ring a step sweeps; a player
/// that comes or goes sweeps its whole circle); a cow that changes cell is tested against the players as well.
/// A test is against the players of the 3 x 3 bins of 32 blocks around the cow, and only the cows that enter or leave
/// the 32 blocks are touched: an entering cow is active without end (`ActiveUntil(u64::MAX)`) and its stroll attempt
/// is armed if it had none; a leaving cow stays active for 100 ticks more (the `noActionTime` it has to count up to).
pub(crate) fn player_near(
    clock: Res<Clock>,
    mut players: ResMut<Players>,
    mut idx: ResMut<Index>,
    mut wheel: ResMut<Wheel>,
    mut entered: ResMut<crate::goals::Entered>,
    mut left: Option<ResMut<crate::despawn::LeftRing>>,
    mut active: Query<(&Bounds, &mut ActiveUntil, Option<&mut Stroll>, Option<&mut Rng>)>,
) {
    const BIN: f64 = 32.0;
    const R2: f64 = BIN * BIN;
    if !players.dirty && idx.crossed.is_empty() {
        return;
    }
    let idx = &mut *idx;
    let pl = &mut *players;
    let origin = idx.origin;
    let bins = ((idx.width as f64 * CELL / BIN).ceil() as i64).max(1) + 2;
    let bin = |v: f64| (((v - origin) / BIN).floor() as i64 + 1).clamp(0, bins - 1);
    let tick = clock.0 as u32 + 1;
    pl.candidates.clear();
    if pl.dirty {
        pl.dirty = false;
        // The bins hold the players as they are now.
        pl.n = bins;
        pl.bins.iter_mut().for_each(Vec::clear);
        pl.bins.resize((bins * bins) as usize, Vec::new());
        for i in 0..pl.list.len() {
            let (px, pz) = pl.list[i];
            pl.bins[(bin(pz) * bins + bin(px)) as usize].push((px, pz));
        }
        std::mem::swap(&mut pl.prev, &mut pl.old);
        pl.prev.clear();
        pl.prev.extend_from_slice(&pl.list);
        // The cells a moved player's two circles do not agree on, each once.
        for i in 0..pl.list.len().max(pl.old.len()) {
            let (n, o) = (pl.list.get(i).copied(), pl.old.get(i).copied());
            if n == o {
                continue;
            }
            let (mut x0, mut x1, mut z0, mut z1) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
            for (px, pz) in [n, o].into_iter().flatten() {
                (x0, x1, z0, z1) = (x0.min(px - BIN), x1.max(px + BIN), z0.min(pz - BIN), z1.max(pz + BIN));
            }
            let (x0, x1) = (idx.column(x0).max(0), idx.column(x1).min(idx.width as i64 - 1));
            let (z0, z1) = (idx.column(z0).max(0), idx.column(z1).min(idx.width as i64 - 1));
            // A rectangle of columns the two circles agree on (inside both, or outside both) holds no animal whose state
            // changes: only the blocks and the columns that hold an animal and are not so are looked at.
            let agree = |cx0: f64, cx1: f64, cz0: f64, cz1: f64| -> bool {
                let (mut inside, mut outside) = (true, true);
                for q in [n, o] {
                    let Some((px, pz)) = q else { inside = false; continue };
                    let (nx, nz) = (px.clamp(cx0, cx1) - px, pz.clamp(cz0, cz1) - pz);
                    let (fx, fz) = ((px - cx0).abs().max((px - cx1).abs()), (pz - cz0).abs().max((pz - cz1).abs()));
                    inside &= fx * fx + fz * fz < R2;
                    outside &= nx * nx + nz * nz >= R2;
                }
                inside || outside
            };
            let b = BLOCK as i64;
            for bz in z0 / b..=z1 / b {
                for bx in x0 / b..=x1 / b {
                    if idx.blocks[(bz * idx.bw as i64 + bx) as usize] == 0 {
                        continue;
                    }
                    let (bx0, bz0) = (origin + (bx * b) as f64 * CELL, origin + (bz * b) as f64 * CELL);
                    if agree(bx0, bx0 + b as f64 * CELL, bz0, bz0 + b as f64 * CELL) {
                        continue;
                    }
                    let mut bits = idx.blocks[(bz * idx.bw as i64 + bx) as usize];
                    while bits != 0 {
                        let k = bits.trailing_zeros() as i64;
                        bits &= bits - 1;
                        let (xx, zz) = (bx * b + k % b, bz * b + k / b);
                        let c = zz as usize * idx.width + xx as usize;
                        if idx.stamp[c] == tick {
                            continue;
                        }
                        let (cx0, cz0) = (origin + xx as f64 * CELL, origin + zz as f64 * CELL);
                        if !agree(cx0, cx0 + CELL, cz0, cz0 + CELL) {
                            idx.stamp[c] = tick;
                            pl.candidates.push(c);
                        }
                    }
                }
            }
        }
    }
    let list_empty = pl.list.is_empty();
    let (bins_n, nbins) = (pl.n, &pl.bins);
    let within = |x: f64, z: f64| {
        let (bx, bz) = (bin(x), bin(z));
        (bz - 1..=bz + 1).any(|zz| {
            (bx - 1..=bx + 1).any(|xx| {
                (0..bins_n).contains(&xx)
                    && (0..bins_n).contains(&zz)
                    && nbins[(zz * bins_n + xx) as usize].iter().any(|&(px, pz)| (x - px).powi(2) + (z - pz).powi(2) < R2)
            })
        })
    };
    let mut test = |e: Entity, active: &mut Query<(&Bounds, &mut ActiveUntil, Option<&mut Stroll>, Option<&mut Rng>)>, wheel: &mut Wheel| {
        let Ok((b, mut a, mut s, mut r)) = active.get_mut(e) else { return };
        let (mn, mx) = (b.0.min_position(), b.0.max_position());
        let now_in = within((mn.x + mx.x) / 2.0, (mn.z + mx.z) / 2.0);
        if now_in && a.0 != u64::MAX {
            a.0 = u64::MAX;
            entered.0.push(e);
            // The first stroll attempt belongs to an entity that walks and has goals; an object only wakes.
            if let (Some(mut s), Some(mut r)) = (s, r)
                && !s.scheduled
                && !s.walking
            {
                arm_stroll(wheel, &clock, e, &mut s, &mut r);
            }
        } else if !now_in && a.0 == u64::MAX {
            a.0 = clock.of(e) + 100;
            if let Some(l) = left.as_mut() {
                l.0.push(e);
            }
        }
    };
    for &c in &pl.candidates {
        for &e in &idx.cells[c] {
            test(e, &mut active, &mut wheel);
        }
    }
    if !list_empty {
        for &(e, cell) in &idx.crossed {
            // A column more or less changes nothing for an animal with no player within a bin of it.
            let (cx, cz) = ((cell as usize % idx.width) as f64 + 0.5 + origin / CELL, (cell as usize / idx.width) as f64 + 0.5 + origin / CELL);
            let (bx, bz) = (bin(cx * CELL), bin(cz * CELL));
            let near = (bz - 1..=bz + 1).any(|zz| (bx - 1..=bx + 1).any(|xx| (0..bins_n).contains(&xx) && (0..bins_n).contains(&zz) && !nbins[(zz * bins_n + xx) as usize].is_empty()));
            if near {
                test(e, &mut active, &mut wheel);
            }
        }
    }
    idx.crossed.clear();
}

pub(crate) fn comfortable_fall_distance(f: f32) -> i32 {
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
    day: Res<crate::hostile::Daylight>,
    mut par: crate::cow::OrdCmds,
    mut q: Query<(Entity, &Pos, &Body, &Contact, &mut Stroll, &mut Nav, Has<InFluid>, &Vel, &NavParams, Has<crate::locomotion::lava::StandsOnLava>, Has<crate::locomotion::climb::Climber>, Has<crate::behaviours::restrict_sun::AvoidSun>), (With<NeedsPath>, Or<(With<crate::locomotion::WalkMode>, With<crate::locomotion::LavaMode>, Without<crate::locomotion::Modes>)>)>,
) {
    let level = &*level;
    let one = |e: Entity, pos: &Pos, body: &Body, c: &Contact, s: &mut Stroll, nav: &mut Nav, fluid: bool, vel: &Vel, sp: &NavParams, lava: bool, climber: bool, sunless: bool| {
        let now = clock.of(e);
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
            stands_on_lava: lava,
        };
        let max_len = sp.follow_range;
        let max_visited = (max_len * 16.0).floor() as i32;
        let target = [s.tx.floor() as i32, s.ty, s.tz.floor() as i32];
        // `GroundPathNavigation.canUpdatePath`: no path is made in the air (on the ground, in a liquid, riding).
        let path = (c.on_ground || fluid)
            .then(|| {
                nav.searches += 1;
                crate::goals::FINDS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if crate::pathsearch::USE_OWN.load(std::sync::atomic::Ordering::Relaxed) {
                    crate::pathsearch::Finder::new(mw, &pm, Evaluator::Walk).find(&[target], max_len, nav.reach, max_visited)
                } else {
                    Finder::new(mw, &pm, Evaluator::Walk).find(&[target], max_len, nav.reach, max_visited)
                }
            })
            .flatten();
        if climber {
            // `WallClimberNavigation.pathToPosition`: the climb goes on straight to the target when the path is over (or when there is none).
            let (to, speed) = (target, nav.speed);
            par.command_scope(e, |mut cmd| {
                cmd.queue(move |w: &mut World| crate::locomotion::climb::wish(w, e, to, speed));
            });
        }
        // `GroundPathNavigation.trimPath` with `avoidSun`: a mob out of the sky's sight keeps only the nodes before the first one the sky sees.
        let path = path.map(|mut p| {
            if sunless && day.0 {
                let at = [pos.0.x.floor() as i32, (pos.0.y + 0.5).floor() as i32, pos.0.z.floor() as i32];
                if !mw.can_see_sky(at[0], at[1], at[2]) {
                    if let Some(i) = p.nodes.iter().position(|n| mw.can_see_sky(n.x, n.y, n.z)) {
                        p.nodes.truncate(i);
                    }
                }
            }
            p
        });
        // `MoveToTargetSink`: a path that does not reach its target starts the clock of the unreachable, one that does clears it.
        if path.as_ref().is_some_and(|p| p.reached && !p.nodes.is_empty()) {
            nav.cant_reach_since = 0;
        } else if nav.cant_reach_since == 0 {
            nav.cant_reach_since = now.max(1);
        }
        par.command_scope(e, |mut cmd| match path {
            Some(p) if !p.nodes.is_empty() => {
                crate::goals::PATH_NODES.fetch_add(p.nodes.len() as u64, std::sync::atomic::Ordering::Relaxed);
                crate::goals::PATHS_FOUND.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
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
    // At most `path_budget` searches (0: no limit): the first of the queue in the table order, the same whatever the thread count. ONE call
    // site of `one` per mode: a third inlined copy slowed the unbudgeted systems by 0.5 % (measured, goal_bench cow idle 1530 -> 1538).
    let mut left = if cfg.path_budget > 0 { cfg.path_budget } else { usize::MAX };
    if cfg.parallel {
        let mut chosen: Vec<Entity> = if cfg.path_budget > 0 { q.iter().map(|x| x.0).take(left).collect() } else { Vec::new() };
        chosen.sort_unstable();
        let (limited, chosen) = (cfg.path_budget > 0, &chosen);
        q.par_iter_mut().for_each(|(e, p, b, c, mut s, mut n, f, v, i, l, cl, u)| {
            if !limited || chosen.binary_search(&e).is_ok() {
                one(e, p, b, c, &mut s, &mut n, f, v, i, l, cl, u);
            }
        });
    } else {
        for (e, p, b, c, mut s, mut n, f, v, i, l, cl, u) in &mut q {
            if left == 0 {
                break;
            }
            left -= 1;
            one(e, p, b, c, &mut s, &mut n, f, v, i, l, cl, u);
        }
    }
    par.flush();
}

/// `PathNavigation.tick` -> `followThePath` (ground): reach the waypoint, or cut the corner, then steer the move
/// control at the next node. (The recompute on a block change is not here.) Visits only the cows that follow a path.
#[allow(clippy::type_complexity)]
pub(crate) fn follow(
    cfg: Res<Config>,
    clock: Res<Clock>,
    level: Res<Level>,
    mut par: crate::cow::OrdCmds,
    mut q: Query<(Entity, &Pos, &Body, &mut Drive, &mut Stroll, &mut Nav, &mut MoveTo), (With<Following>, Or<(With<crate::locomotion::WalkMode>, With<crate::locomotion::LavaMode>, Without<crate::locomotion::Modes>)>)>,
) {
    let level = &*level;
    let one = |e: Entity, pos: &Pos, body: &Body, drive: &mut Drive, s: &mut Stroll, nav: &mut Nav, m: &mut MoveTo| {
        let now = clock.of(e);
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
            par.command_scope(e, |mut cmd| {
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
    par.flush();
}
/// The walks that are over end the goal that asked for them (`goals.rs`), which arms what comes next.
pub(crate) fn path_ended(q: Query<(Entity, &crate::goals::GoalSet, Option<&crate::mount::Mount>), With<PathEnded>>, sets: Query<&crate::goals::GoalSet>, mut reqs: ResMut<crate::goals::Requests>, mut commands: Commands) {
    for (e, g, mount) in &q {
        crate::goals::path_ended(e, g, &mut reqs);
        // The walk of a mount steered by a creature is the creature's walk (`Mob.getNavigation` of a rider).
        if let Some(crate::targeting::Who::Entity(r)) = mount.and_then(|m| m.rider)
            && let Ok(rg) = sets.get(r)
        {
            crate::goals::path_ended(r, rg, &mut reqs);
        }
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

pub(crate) fn wrap(a: f32) -> f32 {
    let mut a = a % 360.0;
    if a >= 180.0 {
        a -= 360.0;
    }
    if a < -180.0 {
        a += 360.0;
    }
    a
}
pub(crate) fn rotlerp(a: f32, b: f32, max: f32) -> f32 {
    let mut r = a + wrap(b - a).clamp(-max, max);
    if r < 0.0 {
        r += 360.0;
    } else if r > 360.0 {
        r -= 360.0;
    }
    r
}
/// `MoveControl.Operation.STRAFE` (`MoveControl.strafe(forwards, right)`): the speed is a quarter of the movement speed; a side
/// step whose target block is not walkable turns into a step forward. The caller (a goal) does it every tick it strafes;
/// the walk of the path (`move_control`) sets the sideways input back to nothing.
pub(crate) fn strafe_drive(level: &Level, pos: &Pos, yaw: &Yaw, body: &Body, d: &mut Drive, forwards: f32, right: f32, walkable: &mut Option<((i32, i32, i32), bool, u64)>, now: u64) {
    let speed = (0.25 * body.speed) as f32;
    let (mut xa, mut za) = (forwards, right);
    let dist = (xa * xa + za * za).sqrt().max(1.0);
    let k = speed / dist;
    (xa, za) = (xa * k, za * k);
    let angle = f64::from(yaw.0 * trig::DEG_TO_RAD);
    let (s, co) = (trig::sin(angle), trig::cos(angle));
    let (dx, dz) = (xa * co - za * s, za * co + xa * s);
    let (x, y, z) = ((pos.0.x + f64::from(dx)).floor() as i32, pos.0.y.floor() as i32, (pos.0.z + f64::from(dz)).floor() as i32);
    // The kind of the block is read once per block and kept for 10 ticks (a strafe step is 0.1 block: the same block for many ticks).
    let ok = match *walkable {
        Some((b, ok, at)) if b == (x, y, z) && now < at + 10 => ok,
        _ => {
            let ok = level.mw.0.path_type_static(x, y, z) == horizon_client_lib::mobs::path::PathType::Walkable;
            *walkable = Some(((x, y, z), ok, now));
            ok
        }
    };
    let (forwards, right) = if ok { (forwards, right) } else { (1.0, 0.0) };
    d.speed = speed;
    d.zza = forwards;
    d.xxa = right;
}
/// `MoveControl.tick`, operation MOVE_TO (the jump over a step is not here: the slab is flat).
pub(crate) fn move_control_one(level: &Level, pos: &Pos, yaw: &mut Yaw, m: &mut MoveTo, d: &mut Drive, body: &Body, landed: bool) {
    const ARRIVED: f64 = 2.500_000_3e-7_f32 as f64;
    d.jump = false;
    d.xxa = 0.0;
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
pub(crate) fn move_control(cfg: Res<Config>, level: Res<Level>, mut q: Query<(&Pos, &mut Yaw, &mut MoveTo, &mut Drive, &Body, &Contact, &FluidState), (Or<(With<Following>, With<crate::locomotion::climb::Climbing>)>, Or<(With<crate::locomotion::WalkMode>, With<crate::locomotion::LavaMode>, Without<crate::locomotion::Modes>)>)>) {
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
pub(crate) fn move_relative(vel: &mut Vel, yaw: f32, speed: f32, xxa: f32, zza: f32) {
    let input = Vec3::new(f64::from(xxa), 0.0, f64::from(zza));
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
pub(crate) fn do_move(level: &Level, pos: &mut Pos, vel: &mut Vel, c: &mut Contact, body: &Body, bounds: &mut Bounds, fall_damage: &mut f32) {
    let wanted = vel.0;
    let p = pos.0;
    let subject = body.lava_walker.then(|| horizon_client_render::collisions::PowderSnowSubject { lava_walker: true, ..Default::default() });
    let scene = SceneCollisions { world: level.mw.0.world, binding: level.mw.0.collision, subject };
    let moved = level.occ.resolve(scene, bounds.0, wanted, body.step_height, c.on_ground).unwrap_or(Vec3::ZERO);
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
/// `ItemEntity.tick`'s motion for an object: a fluid slows it (water 0.99, lava 0.95, lifting a slow one by 5.0E-4) or gravity pulls it
/// down BEFORE the move; it moves unless it lies still on the ground; then the drag of the air (0.98 on every axis, times the block's friction
/// on the ground for x and z) and the bounce (a downward speed on the ground is halved and turned up). True when it is at rest. An object that
/// walks (`Drive`) is accelerated as a creature is, before the move.
#[allow(clippy::too_many_arguments)]
fn travel_object(level: &Level, pos: &mut Pos, vel: &mut Vel, yaw: &Yaw, c: &mut Contact, d: &mut Drive, body: &Body, bounds: &mut Bounds, fs: &FluidState, fall_damage: &mut f32) -> bool {
    if body.orb {
        return travel_orb(level, pos, vel, c, d, body, bounds, fs, fall_damage);
    }
    let height = f64::from(0.1_f32);
    let (in_water, in_lava) = (fs.water > height, fs.lava > height);
    if in_water || in_lava {
        let drag = f64::from(if in_water { 0.99_f32 } else { 0.95_f32 });
        let lift = if vel.0.y < f64::from(0.06_f32) { f64::from(5.0e-4_f32) } else { 0.0 };
        vel.0 = Vec3::new(vel.0.x * drag, vel.0.y + lift, vel.0.z * drag);
    } else {
        vel.0.y -= body.gravity;
    }
    if d.zza != 0.0 || d.xxa != 0.0 {
        let speed = if c.on_ground { d.speed } else { 0.02 };
        move_relative(vel, yaw.0, speed, d.xxa, d.zza);
    }
    let horizontal = vel.0.x * vel.0.x + vel.0.z * vel.0.z;
    if c.on_ground && horizontal <= 1.0e-5 && d.zza == 0.0 && d.xxa == 0.0 && vel.0.y <= 0.0 && vel.0.y > -body.gravity * 1.01 {
        // Lying still on the ground: the gravity of this tick is not integrated, nothing moves.
        vel.0 = Vec3::ZERO;
        return true;
    }
    do_move(level, pos, vel, c, body, bounds, fall_damage);
    let mut friction = 0.98_f32;
    if c.on_ground {
        let mw = level.mw.0;
        let below = mw.world.block(pos.0.x.floor() as i32, (pos.0.y - 0.999_999).floor() as i32, pos.0.z.floor() as i32);
        friction *= mw.facts.get(below).friction;
    }
    let (f, air) = (f64::from(friction), f64::from(0.98_f32));
    vel.0 = Vec3::new(vel.0.x * f, vel.0.y * air, vel.0.z * f);
    if c.on_ground && vel.0.y < 0.0 {
        vel.0.y *= -0.5;
    }
    false
}
/// `ExperienceOrb.tick`'s motion: its eye in water floats it (0.99, lifted by 5.0E-4 up to 0.06), else gravity unless it is in a block; it moves,
/// then every axis loses the same drag (0.98, times the block's friction on the ground), and a fall faster than 0.03 bounces by 0.4 of it.
#[allow(clippy::too_many_arguments)]
fn travel_orb(level: &Level, pos: &mut Pos, vel: &mut Vel, c: &mut Contact, d: &mut Drive, body: &Body, bounds: &mut Bounds, fs: &FluidState, fall_damage: &mut f32) -> bool {
    let mw = level.mw.0;
    if fs.eye_in_water {
        vel.0 = Vec3::new(vel.0.x * f64::from(0.99_f32), (vel.0.y + f64::from(5.0e-4_f32)).min(f64::from(0.06_f32)), vel.0.z * f64::from(0.99_f32));
    } else if level.no_collision(bounds.0) {
        vel.0.y -= body.gravity;
    }
    let horizontal = vel.0.x * vel.0.x + vel.0.z * vel.0.z;
    if c.on_ground && !fs.in_water && horizontal <= 1.0e-10 && vel.0.y <= 0.0 && vel.0.y > -body.gravity * 1.01 {
        vel.0 = Vec3::ZERO;
        return true;
    }
    let wanted_y = vel.0.y;
    do_move(level, pos, vel, c, body, bounds, fall_damage);
    let mut friction = 0.98_f32;
    if c.on_ground {
        let below = mw.world.block(pos.0.x.floor() as i32, (pos.0.y - 0.999_999).floor() as i32, pos.0.z.floor() as i32);
        friction *= mw.facts.get(below).friction;
    }
    let f = f64::from(friction);
    vel.0 = Vec3::new(vel.0.x * f, vel.0.y * f, vel.0.z * f);
    if c.on_ground && wanted_y < -0.03 {
        vel.0.y = -wanted_y * 0.4;
    }
    let _ = d;
    false
}
/// `LivingEntity.travel` of a cow: in the air (on the ground), or in water or lava (`travelInFluid`).
#[allow(clippy::too_many_arguments)]
pub(crate) fn travel_one(
    level: &Level,
    sp: &FluidParams,
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
    if body.object {
        return travel_object(level, pos, vel, yaw, c, d, body, bounds, fs, fall_damage);
    }
    let rest_vy = (0.0 - body.gravity) * f64::from(0.98_f32);
    let in_fluid = fs.in_water || fs.in_lava;
    let jumping = jumping || d.jump;
    // A cow at its resting state stays as it is: no sweep is made for it.
    if !in_fluid && !jumping && c.on_ground && d.levitation == 0 && d.zza == 0.0 && d.xxa == 0.0 && vel.0.x == 0.0 && vel.0.z == 0.0 && vel.0.y == rest_vy {
        return true;
    }
    // `Chicken.aiStep`: a hen in the air has its downward speed cut (the species' `slow_fall`, 1 for the others).
    if !c.on_ground && vel.0.y < 0.0 {
        vel.0.y *= body.fall_factor;
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
            move_relative(vel, yaw.0, speed, d.xxa, d.zza);
            do_move(level, pos, vel, c, body, bounds, fall_damage);
            let m = vel.0;
            let m = Vec3::new(m.x * f64::from(slow), m.y * f64::from(0.8_f32), m.z * f64::from(slow));
            vel.0 = horizon_client_lib::mobs::living::fluid_falling(body.gravity, falling, false, m);
            c.fall_distance = 0.0;
        } else {
            move_relative(vel, yaw.0, 0.02, d.xxa, d.zza);
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
        if c.horizontal && level.no_collision(ahead) && !mw.contains_any_liquid(ahead) {
            vel.0.y = f64::from(0.3_f32);
        }
        return false;
    }
    let friction: f32 = if c.on_ground { 0.6 } else { 1.0 };
    let speed = if c.on_ground {
        if friction > 0.6 { d.speed * (0.216_000_02 / (friction * friction * friction)) } else { d.speed }
    } else {
        d.air_speed.unwrap_or(0.02)
    };
    move_relative(vel, yaw.0, speed, d.xxa, d.zza);
    // `LivingEntity.handleOnClimbable`, the move, then the climb up a wall (a wall climber pressed against one).
    let climbing = d.climber && d.climbing;
    if climbing {
        c.fall_distance = 0.0;
        let limit = f64::from(0.15_f32);
        let u = vel.0;
        vel.0 = Vec3::new(u.x.clamp(-limit, limit), if u.y.is_nan() { u.y } else { u.y.max(-limit) }, u.z.clamp(-limit, limit));
    }
    do_move(level, pos, vel, c, body, bounds, fall_damage);
    let mut movement = vel.0;
    if climbing && (c.horizontal || jumping) {
        movement.y = 0.2;
    }
    if d.climber {
        // `Spider.tick`: climbing while pressed against a wall.
        d.climbing = c.horizontal;
    }
    // `LivingEntity.travelInAir`: levitation (`effects.rs`) pulls the vertical speed toward 0.05 * (level + 1) instead of gravity.
    let y = if d.levitation > 0 {
        c.fall_distance = 0.0;
        movement.y + (0.05 * f64::from(d.levitation) - movement.y) * 0.2
    } else {
        movement.y - body.gravity
    };
    if d.discard_friction {
        vel.0 = Vec3::new(movement.x, y, movement.z);
        return false;
    }
    let drag = 0.91_f32;
    let f = friction * drag;
    vel.0 = Vec3::new(movement.x * f64::from(f), y * f64::from(0.98_f32), movement.z * f64::from(f));
    !jumping && c.on_ground && d.zza == 0.0 && d.xxa == 0.0 && vel.0.x == 0.0 && vel.0.z == 0.0 && vel.0.y == rest_vy
}
#[allow(clippy::type_complexity)]
pub(crate) fn travel(
    cfg: Res<Config>,
    level: Res<Level>,
    mut par: crate::cow::OrdCmds,
    plain: ParallelCommands,
    mut q: Query<
        (Entity, &mut Pos, &mut Vel, Option<&Yaw>, &mut Contact, Option<&mut Drive>, &Body, &mut Bounds, &FluidState, Has<Following>, Option<&crate::goals::Jump>, Option<&FluidParams>, Option<&Immune>),
        (With<Moving>, Without<Overlapping>, Without<crate::mount::Riding>, Or<(With<crate::locomotion::WalkMode>, Without<crate::locomotion::Modes>)>),
    >,
) {
    let void_y = f64::from(level.mw.0.world.bottom_y() - 64);
    let one = |e: Entity, mut p: Mut<Pos>, mut v: Mut<Vel>, y: Option<&Yaw>, mut c: Mut<Contact>, d: Option<Mut<Drive>>, b: &Body, mut bb: Mut<Bounds>, fs: &FluidState, following: bool, jump: Option<&crate::goals::Jump>, fp: Option<&FluidParams>, imm: Option<&Immune>| {
        let mut fall = 0.0_f32;
        let jumping = jump.is_some_and(|j| j.0);
        // A body without walking (an object) has no yaw and no input.
        let mut none = Drive::default();
        let mut d = d;
        let dr: &mut Drive = match d.as_mut() {
            Some(d) => d,
            None => &mut none,
        };
        let was_in_air = !c.on_ground;
        let was_on_ground = c.on_ground;
        let rest = travel_one(&level, fp.unwrap_or(&FluidParams::DEFAULT), &mut p, &mut v, y.unwrap_or(&Yaw(0.0)), &mut c, dr, b, &mut bb, fs, jumping, &mut fall);
        // The events of landing are the goals' (an object has none: an orb that glides over the ground would queue two a tick).
        if was_on_ground && !c.on_ground && !b.object {
            par.command_scope(e, |mut cmd| {
                cmd.queue(move |w: &mut World| w.resource_mut::<crate::goals::Events>().left_ground.push(e));
            });
        }
        if was_in_air && c.on_ground && !b.object {
            par.command_scope(e, |mut cmd| {
                cmd.queue(move |w: &mut World| w.resource_mut::<crate::goals::Events>().landed.push(e));
            });
        }
        if fall > 0.0 && !b.object && !imm.is_some_and(|i| i.has(IMMUNE_FALL)) {
            par.command_scope(e, |mut cmd| {
                cmd.queue(move |w: &mut World| w.resource_mut::<DamageQueue>().add(e, fall, Cause::Fall));
            });
        }
        // `Entity.checkBelowWorld`: under the world a creature is hurt 4 a tick (it dies: it does not fall for ever).
        if p.0.y < void_y {
            par.command_scope(e, |mut cmd| {
                cmd.queue(move |w: &mut World| {
                    // An experience orb is not alive: it is discarded (`Entity.checkBelowWorld`), a creature is hurt.
                    if w.get::<crate::orb::Orb>(e).is_some() {
                        w.despawn(e);
                    } else {
                        w.resource_mut::<DamageQueue>().add(e, 4.0, Cause::OutOfWorld);
                    }
                });
            });
        }
        // At rest and not following a path: the physics of the cow goes inactive until something moves it.
        if rest && !following {
            // Not commutative across threads: the entity takes the last row of the table it lands in (or the swap of a sparse set), and the
            // systems that walk it see that order. A parallel visit therefore sorts by emitter; the sequential visit is already in order, and
            // the ordered buffer costs a queue per entity (measured: zombie chase clear +10k instr per tick).
            // The path budget takes "the first of the queue in the table order": it needs the same order as the parallel run, so it is ordered too.
            if cfg.parallel || cfg.path_budget > 0 {
                par.command_scope(e, |mut cmd| {
                    cmd.entity(e).remove::<Moving>();
                });
            } else {
                plain.command_scope(|mut cmd| {
                    cmd.entity(e).remove::<Moving>();
                });
            }
        }
    };
    if cfg.parallel {
        q.par_iter_mut().for_each(|(e, p, v, y, c, d, b, bb, fs, f, j, i, m)| one(e, p, v, y, c, d, b, bb, fs, f, j, i, m));
    } else {
        for (e, p, v, y, c, d, b, bb, fs, f, j, i, m) in &mut q {
            one(e, p, v, y, c, d, b, bb, fs, f, j, i, m);
        }
    }
    par.flush();
}

/// `Entity.updateFluidInteraction` (Horizon's real fluid heights and currents), only for the cows whose box overlaps
/// a column with a fluid.
pub(crate) fn fluid_update(
    cfg: Res<Config>,
    level: Res<Level>,
    mut q: Query<(&Pos, &Bounds, &mut Vel, &mut FluidState, &mut Contact, Option<&FluidParams>), With<InFluid>>,
) {
    let level = &*level;
    let one = |pos: &Pos, b: &Bounds, v: &mut Vel, fs: &mut FluidState, c: &mut Contact, fp: Option<&FluidParams>| {
        let mw = level.mw.0;
        let f = mw.fluids(b.0, pos.0.y + fp.unwrap_or(&FluidParams::DEFAULT).eye_height);
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
    mut q: Query<(Entity, &Pos, &FluidState, &mut Air, Has<AirLow>, Has<InFluid>, &FluidParams, &Immune, &TypeName, &crate::goals::Uid, Has<crate::behaviours::ender_teleport::WaterSensitive>), Or<(With<InFluid>, With<AirLow>)>>,
    mut queue: ResMut<DamageQueue>,
    mut commands: Commands,
) {
    let mw = level.mw.0;
    for (e, pos, fs, mut air, low, in_fluid, fp, imm, tn, uid, wet) in &mut q {
        // `Enderman.aiStep`: the water hurts it (1 a tick, through the invulnerability window), and the hurt sends it away.
        if wet && fs.in_water {
            queue.add(e, 1.0, Cause::Drown);
        }
        let submerged = fs.eye_in_water && {
            let (x, y, z) = (pos.0.x.floor() as i32, (pos.0.y + fp.eye_height).floor() as i32, pos.0.z.floor() as i32);
            mw.block_name(x, y, z) != "minecraft:bubble_column"
        };
        let drowns = submerged && !imm.has(BREATHES_WATER);
        let vanilla = horizon_client_lib::breathing::Breath {
            can_breathe: !drowns,
            consume: 1,
            refill: if drowns { 0 } else { 4 },
        };
        let id = uid.0 as u32;
        let breath = horizon_client_lib::breathing::breathe(mw.loop_context, id, tn.0, vanilla);
        if let Some(d) = horizon_client_lib::breathing::drown(mw.loop_context, id, tn.0, &mut air.0, 300, breath, true)
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
pub(crate) fn wall_check(
    level: Res<Level>,
    q: Query<(Entity, &Pos, &Body, Has<InWall>, &FluidParams), With<Recheck>>,
    mut commands: Commands,
) {
    use horizon_core::geometry::{ShapeOp, VoxelShape};
    let mw = level.mw.0;
    for (e, pos, body, was, sp) in &q {
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
    mut events: ResMut<crate::goals::Events>,
    mut q: Query<(Entity, &Bounds, &mut Cell, Option<&Hazard>, Has<InFluid>, &mut FluidState), Changed<Bounds>>,
    // Read only when the entity enters or leaves a fluid (rare): not part of the query that runs over every moved entity.
    all_modes: Query<&crate::locomotion::Modes>,
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
            idx.move_watcher(e, c.0, cell);
            c.0 = cell;
            moves.0.push(e);
            idx.crossed.push((e, cell));
        }
        idx.set_box(e, cell, (mn.x, mn.z), (mx.x, mx.z));
        let bits = hazards.of_box(&b.0);
        let fluid = bits & (HAZ_WATER | HAZ_LAVA) != 0;
        if fluid && !in_fluid {
            // A cow that enters a fluid (placed water, a push) needs its physics at once.
            commands.entity(e).insert((InFluid, Moving));
            events.entered_fluid.push(e);
            // The mode of the water, if the entity has one, becomes the active one: an event, not a test each tick.
            if let Ok(m) = all_modes.get(e) {
                crate::locomotion::fluid_changed(&mut commands, e, m, bits & HAZ_WATER != 0, bits & HAZ_LAVA != 0);
            }
        } else if !fluid && in_fluid {
            commands.entity(e).remove::<InFluid>();
            events.left_fluid.push(e);
            if let Ok(m) = all_modes.get(e) {
                crate::locomotion::fluid_changed(&mut commands, e, m, false, false);
            }
            *fs = FluidState::default();
        }
        let was = hz.is_some_and(|h| h.0 & HAZ_SNOW != 0);
        let is = bits & HAZ_SNOW != 0;
        if is && !was {
            events.entered_powder_snow.push(e);
        } else if was && !is {
            events.left_powder_snow.push(e);
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
    mut queue: ResMut<DamageQueue>,
    mut idx: ResMut<Index>,
    mut wheel: ResMut<Wheel>,
    mut q: Query<(&mut Health, &mut HurtAt, &Cell, &Pos, &mut Vel, &Contact, Option<&mut Rng>, Option<&Resist>, Option<&Immune>, Option<&crate::equipment::Armor>, Option<&SpeciesId>)>,
    species: Res<SpeciesTable>,
    mut hurts: ResMut<crate::goals::Hurts>,
    mut ps: ResMut<crate::hostile::PlayerState>,
    mut by: ResMut<crate::hostile::HurtBy>,
    mut causes: ResMut<HurtCauses>,
    mut events: ResMut<crate::goals::Events>,
    mut commands: Commands,
) {
    // The causes of the previous step were read by the goals (they run after this system in the chain).
    causes.0.clear();
    for hit in std::mem::take(&mut queue.0) {
        let e = hit.target;
        let Ok((mut h, mut hurt, cell, pos, mut v, c, rng, resist, immune, armor, sid)) = q.get_mut(e) else {
            continue;
        };
        let now = clock.of(e);
        let cooldown = now < hurt.at + 10 && hurt.at != 0 && !immune.is_some_and(|i| i.has(NO_HURT_COOLDOWN));
        let mut applied = if cooldown {
            if hit.amount <= hurt.last {
                continue;
            }
            hit.amount - hurt.last
        } else {
            hit.amount
        };
        // `LivingEntity.getDamageAfterArmorAbsorb`: what the worn armor stops, unless the hurt is in `#bypasses_armor`.
        if crate::equipment::armor_applies(hit.cause) {
            let (a, t) = armor.map_or_else(|| sid.map_or((0.0, 0.0), |s| crate::equipment::base_armor(&species.0[s.0 as usize])), |a| (a.armor, a.toughness));
            applied = crate::equipment::absorb(applied, a, t);
        }
        *hurt = HurtAt { at: now, last: hit.amount };
        h.0 -= applied;
        if h.0 <= 0.0 {
            if cell.0 != u32::MAX {
                idx.remove(e, cell.0);
            }
            // What it leaves on the ground (`LivingEntity.die`), then it goes.
            commands.queue(move |w: &mut World| {
                crate::loot::on_death(w, e);
                crate::spawn_trial::forget(w, e);
                if let Ok(mut m) = w.get_entity_mut(e) {
                    m.despawn();
                }
            });
            continue;
        }
        hurts.0.push(e);
        events.hurt.push(e);
        events.vibration.push(crate::behaviours::vibration_listen::Vibration { pos: [pos.0.x, pos.0.y, pos.0.z], source: hit.by, owner: None, projectile: false });
        if hit.by.is_some() {
            events.hurt_by_entity.push(e);
        }
        causes.0.push((e, hit.cause));
        commands.entity(e).insert(crate::brain::LastDamage { at: clock.of(e), panic: !matches!(hit.cause, Cause::Fall | Cause::Drown | Cause::InWall | Cause::Cramming), by: hit.by });
        if let Some(p) = hit.by {
            by.0.push((e, p));
            // `Player.setLastHurtMob`: what a player hit last, for the pets that attack it.
            if let crate::targeting::Who::Player(id) = p {
                ps.set_last_hurt(id, crate::targeting::Who::Entity(e), clock.0);
            }
        }
        // The reactions of the modules to the hurt (`Animal.actuallyHurt` ends love, `after_hurt` resets `noActionTime`...): each
        // module of the entity has its `damaged` entry point; the frame does not know what they do.
        // `getLastHurtByMob`: who hurt it last, readable by every module (`ResetUniversalAngerTarget`, revenge...), with its own time.
        if let Some(who) = hit.by {
            commands.entity(e).insert(LastHurtBy { who, at: clock.of(e) });
        }
        let (who, direct) = (hit.by, hit.direct);
        commands.queue(move |w: &mut World| {
            crate::goals::dispatch_damaged(w, e, &crate::behaviours::Damaged { amount: applied, who, direct });
            crate::despawn::touch(w, e);
        });
        // `dealDefaultKnockback`: an entity's hit outside the cooldown (the other causes are in `#no_knockback`).
        if hit.cause == Cause::Entity && !cooldown {
            let power = 0.4_f64 * (1.0 - resist.map_or(0.0, |r| r.0));
            if power > 0.0 {
                let (mut dx, mut dz) = hit.from.map_or((0.0, 0.0), |f| (f.0 - pos.0.x, f.1 - pos.0.z));
                let mut spare = Rng(clock.0 | 1);
                let mut rng = rng;
                while dx * dx + dz * dz < f64::from(1.0e-5_f32) {
                    let r: &mut Rng = match rng.as_mut() {
                        Some(r) => r,
                        None => &mut spare,
                    };
                    dx = (r.next_f64() - r.next_f64()) * 0.01;
                    dz = (r.next_f64() - r.next_f64()) * 0.01;
                }
                let l = (dx * dx + dz * dz).sqrt();
                let (kx, kz) = (dx / l * power, dz / l * power);
                let y = if c.on_ground { (v.0.y / 2.0 + power).min(0.4) } else { v.0.y };
                v.0 = Vec3::new(v.0.x / 2.0 - kx, y, v.0.z / 2.0 - kz);
                commands.entity(e).insert(Moving);
            }
        }
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
    mut events: ResMut<crate::goals::Events>,
    mut q: Query<(Entity, &Hazard, &mut Frozen, Option<&Burning>, &Pos, &Bounds, &FluidParams)>,
    mut commands: Commands,
) {
    let mw = level.mw.0;
    for (e, h, mut frozen, burning, pos, b, sid) in &mut q {
        let now = clock.of(e);
        let f = h.0;
        // Water and lava as the `Mob` reads them: the fluid height at the cow's box, not the column.
        let (in_water, in_lava) = if f & (HAZ_WATER | HAZ_LAVA) != 0 {
            let r = mw.fluids(b.0, pos.0.y + sid.eye_height);
            (r.in_water, r.in_lava)
        } else {
            (false, false)
        };
        let lit = f & HAZ_FIRE != 0 || in_lava;
        if in_water {
            if burning.is_some() {
                commands.entity(e).remove::<Burning>();
                events.extinguished.push(e);
            }
        } else {
            let seconds = if in_lava { 300 } else if lit { 160 } else { 0 };
            if seconds > 0 {
                let until = now + seconds;
                match burning {
                    None => {
                        // `Entity.baseTick` hurts when the time left is a multiple of 20, the first tick included.
                        wheel.push(clock.at(e, now + if seconds % 20 == 0 { 1 } else { 20 }), e, Due::Burn);
                        commands.entity(e).insert(Burning { until });
                        events.ignited.push(e);
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
/// The commands collected by `OrdCmds` on one thread (with the emitting entity) and the empty queues kept for the next scopes (no allocation per scope).
#[derive(Default)]
pub struct Pending {
    done: Vec<(Entity, bevy_ecs::world::CommandQueue)>,
    spare: Vec<bevy_ecs::world::CommandQueue>,
}
/// `ParallelCommands` with a DETERMINISTIC order: the commands each entity's visit emits are kept with the entity, and `flush` (at the end of
/// the system) applies them sorted by entity, whatever thread ran the visit. The order commands are applied in decides the rows of the tables
/// (an entity that changes table takes the last row), hence the iteration order of every later system: with the per-thread queues of
/// `ParallelCommands` two runs, or two thread counts, gave different rows and so different games (suivi/ecs-echelle.md, G').
/// One `Pending` per thread (no lock: the first version took a mutex twice per visit, +50 instr per visit measured).
#[derive(bevy_ecs::system::SystemParam)]
pub struct OrdCmds<'w, 's> {
    pending: Local<'s, bevy_utils::Parallel<Pending>>,
    gathered: Local<'s, Vec<(Entity, bevy_ecs::world::CommandQueue)>>,
    par: ParallelCommands<'w, 's>,
    allocator: &'w bevy_ecs::entity::EntityAllocator,
    entities: &'w bevy_ecs::entity::Entities,
}
impl OrdCmds<'_, '_> {
    /// Commands emitted by the visit of `by` (the sort key); callable from the threads of a parallel iteration.
    pub fn command_scope(&self, by: Entity, f: impl FnOnce(Commands)) {
        self.pending.scope(|p| {
            let mut q = p.spare.pop().unwrap_or_default();
            f(Commands::new_from_entities(&mut q, self.allocator, self.entities));
            if q.is_empty() {
                p.spare.push(q);
            } else {
                p.done.push((by, q));
            }
        });
    }
    /// Hands the collected commands to the system's buffer, sorted by emitter (stable: one entity's commands keep their order).
    pub fn flush(&mut self) {
        let all = &mut *self.gathered;
        for p in self.pending.iter_mut() {
            all.append(&mut p.done);
        }
        if all.is_empty() {
            return;
        }
        // Sorted by emitter; the sort is stable and each thread's list is in its visit order, so one entity's commands keep their order.
        all.sort_by_key(|c| c.0);
        self.par.command_scope(|mut cmd| {
            for (_, q) in all.iter_mut() {
                cmd.append(q);
            }
        });
        // The emptied queues go back to the threads' pools (round robin), their allocations kept.
        let mut pools: Vec<&mut Vec<bevy_ecs::world::CommandQueue>> = self.pending.iter_mut().map(|p| &mut p.spare).collect();
        let n = pools.len().max(1);
        for (i, (_, q)) in all.drain(..).enumerate() {
            pools[i % n].push(q);
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
        } else if clock.of(e).saturating_sub(nav.last_recompute) <= 20 || !(c.on_ground || fs.in_water || fs.in_lava) {
            nav.delayed = true;
        } else {
            nav.last_recompute = clock.of(e);
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
pub(crate) fn block_changes(
    mut changes: ResMut<BlockChanges>,
    mut state_changes: ResMut<StateChanges>,
    level: Res<Level>,
    mut hazards: ResMut<Hazards>,
    idx: Res<Index>,
    mut sun: ResMut<crate::hostile::SunDirty>,
    mut seen: ResMut<crate::hostile::ChangedBlocks>,
    mut dirty: Option<ResMut<crate::spawning::DirtyColumns>>,
    mut q: Query<(&mut Bounds, &Pos, &mut Contact)>,
    mut paths: Query<(Entity, &Pos, &Stroll, &mut Nav), With<Following>>,
    mut commands: Commands,
) {
    if changes.0.is_empty() && state_changes.0.is_empty() && state_changes.1.is_empty() {
        return;
    }
    let mw = level.mw.0;
    let named = std::mem::take(&mut changes.0).into_iter().map(|(x, y, z, n)| (x, y, z, n, &[][..], None));
    let with_state = std::mem::take(&mut state_changes.0).into_iter().map(|(x, y, z, n, p)| (x, y, z, n, p, None));
    let by_id = std::mem::take(&mut state_changes.1).into_iter().map(|(x, y, z, id)| (x, y, z, "", &[][..], Some(id)));
    for (x, y, z, name, props, id) in named.chain(with_state).chain(by_id).collect::<Vec<_>>() {
        let Some(state) = id.or_else(|| crate::blocks::resolve(mw, name, props)) else { continue };
        let name: &str = if id.is_some() { mw.registry.state(state).map_or("", |s| s.name.as_str()) } else { name };
        // SAFETY: this system runs alone in the chain, no reference to the blocks is alive meanwhile.
        unsafe {
            let world = &mut *level.blocks.0;
            let _ = world.set(x, y, z, state);
            level.occ.changed(&*world, level.mw.0.collision, x, y, z);
            world.path_type_block_updated(horizon_core::position::BlockPos::new(x, y, z));
            // The real light engine: the sky above a column changed with the block.
            horizon_client_lib::mobs::world::relight_test_world(world, &[(x, y, z)]);
        }
        hazards.set(x, y, z, hazard_bit(name));
        hazards.eggs.remove(&[x, y, z]);
        if name == EGG {
            hazards.eggs.insert([x, y, z]);
        }
        seen.0.push((x, y, z));
        if let Some(d) = dirty.as_mut() {
            d.0.push((x, z));
        }
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
    mut par: crate::cow::OrdCmds,
    // The neighbours include the creatures asleep: a `Mob` that is not ticked is still in the snapshot, it pushes the ones that
    // overlap it and is pushed by them (`Entity.push` moves both), only it does not move until it is ticked again.
    boxes: Query<(Ref<Bounds>, Has<Disabled>), Allow<Disabled>>,
    objects: Query<&Body>,
    ordered: Query<(), With<Overlapping>>,
    touching: Query<(), With<Touching>>,
    mut movers: Query<(Entity, &Bounds, &mut Vel, Has<Moving>, Has<Touching>), (Or<(Changed<Bounds>, With<Touching>)>, Without<Overlapping>, Without<PushFree>)>,
) {
    // `Entity.isPushable`: an object (an item on the ground) neither pushes nor is pushed; read only where two boxes overlap.
    let is_object = |e: Entity| objects.get(e).is_ok_and(|b| b.object);
    let one = |e: Entity, b: &Bounds, v: &mut Vel, moving: bool, was_touching: bool| {
        let (mn, mx) = (b.0.min_position(), b.0.max_position());
        let (cx, cz) = ((mn.x + mx.x) / 2.0, (mn.z + mx.z) / 2.0);
        let mut dv = (0.0_f64, 0.0_f64);
        let mut overlapping = 0;
        // An object (an item on the ground) neither pushes nor is pushed: it scans nothing.
        let e_object = is_object(e);
        if !e_object {
        idx.for_near_touching(cx, cz, (mn.x, mn.z), (mx.x, mx.z), |o| {
            if o == e {
                return;
            }
            let Ok((ob, asleep)) = boxes.get(o) else { return };
            if !b.0.intersects(ob.0) || is_object(o) {
                return;
            }
            overlapping += 1;
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
            // `Entity.push` is called for the pair from both sides every tick (each entity runs `pushEntities`): each of the
            // two receives its impulse twice per tick.
            let (bx, bz) = (xa * pow * f64::from(0.05_f32), za * pow * f64::from(0.05_f32));
            // The neighbour `o` is ticked before this animal (a lower index) or after it.
            let lower = o.index().index() < e.index().index();
            let gain = |o_lower: bool| if cfg.push_form == 0 { 2.0 * cfg.push_k } else { 1.0 + if o_lower { cfg.push_k } else { 1.0 } };
            let (gx, gz) = (bx * gain(lower), bz * gain(lower));
            // What the neighbour receives of this animal's call: its own call is the other one.
            xa = bx * gain(!lower);
            za = bz * gain(!lower);
            dv.0 -= gx;
            dv.1 -= gz;
            // An overlap begins: in the ordered mode this one and its neighbour are handled by `ordered_pass` from the
            // next tick (this tick's impulse was given above).
            if cfg.ordered_push {
                par.command_scope(e, |mut cmd| {
                    cmd.entity(e).insert(Overlapping);
                    cmd.entity(o).insert(Overlapping);
                });
            }
            if cfg.keep_touching {
                par.command_scope(e, |mut cmd| {
                    cmd.entity(o).insert(Touching);
                });
            }
            if (!ob.is_changed() && !(cfg.keep_touching && touching.contains(o))) || ordered.contains(o) {
                par.command_scope(e, |mut cmd| {
                    // A sleeper keeps the push (its velocity changes) and moves when it wakes, not before.
                    let (push, wake) = (Vec3::new(xa, 0.0, za), !asleep);
                    cmd.queue(move |w: &mut World| add_push(w, o, push, wake));
                });
            }
        });
        }
        if cfg.keep_touching {
            if overlapping > 0 {
                par.command_scope(e, |mut cmd| {
                    cmd.entity(e).insert(Touching);
                });
            } else if was_touching {
                par.command_scope(e, |mut cmd| {
                    cmd.entity(e).remove::<Touching>();
                });
            }
        }
        // A pile: this one and the ones it overlaps are looked at by the cramming module (each tick until they are not).
        if overlapping + 1 >= MAX_ENTITY_CRAMMING {
            par.command_scope(e, |mut cmd| {
                cmd.entity(e).insert(Crowded);
            });
            idx.for_near(cx, cz, |o| {
                if o != e && boxes.get(o).is_ok_and(|(ob, _)| !is_object(o) && b.0.intersects(ob.0)) {
                    par.command_scope(e, |mut cmd| {
                        cmd.entity(o).insert(Crowded);
                    });
                }
            });
        }
        if dv != (0.0, 0.0) {
            v.0 = Vec3::new(v.0.x + dv.0, v.0.y, v.0.z + dv.1);
            if !moving {
                par.command_scope(e, |mut cmd| {
                    cmd.entity(e).insert(Moving);
                });
            }
        }
    };
    if cfg.parallel {
        movers.par_iter_mut().for_each(|(e, b, mut v, m, t)| one(e, b, &mut v, m, t));
    } else {
        for (e, b, mut v, m, t) in &mut movers {
            one(e, b, &mut v, m, t);
        }
    }
    par.flush();
}
/// How near (blocks) another animal must come for an animal to stay in the ordered pass: a little more than the largest
/// step of an animal in a tick.
const ORDERED_MARGIN: f64 = 0.1;
/// Two animals whose boxes come within this distance (blocks) of each other in the ordered pass are in one island: more
/// than the two largest steps of a tick (0.6 each).
const ISLAND_REACH: f64 = 1.2;
fn island_reach() -> f64 {
    static R: std::sync::OnceLock<f64> = std::sync::OnceLock::new();
    *R.get_or_init(|| std::env::var("ISLAND_REACH").ok().and_then(|v| v.parse().ok()).unwrap_or(ISLAND_REACH))
}
/// An animal that overlapped another when `push` last looked at it (batch mode, `Config.keep_touching`): it is looked at
/// again each tick, moving or not, until nothing overlaps it.
#[derive(Component, Default)]
#[component(storage = "SparseSet")]
pub struct Touching;
/// An animal that overlaps at least one other (ordered push mode): its travel and its push run in `ordered_pass`.
#[derive(Component, Default)]
#[component(storage = "SparseSet")]
pub struct Overlapping;
/// How many animals the ordered pass handled: the last tick, and the sum over the ticks (for the benches).
#[derive(Resource, Default)]
pub struct OrderedStats {
    pub last: usize,
    pub total: u64,
    pub ticks: u64,
    /// Islands over the ticks, the largest one, and how many islands had 1, 2-3, 4-10, 11-100, more than 100 animals.
    pub islands: u64,
    pub largest: usize,
    pub sizes: [u64; 5],
}
/// The ordered push mode (`Config.ordered_push`): the animals that overlap another travel and push in ONE pass, in the
/// order of their creation, with the `Mob` pass's semantics: an animal travels, then pushes every animal that overlaps
/// it (its own velocity and the other's at once); the impulse an animal gets from one that comes before it is used by
/// its own travel of this tick (the `Mob` delivers the event at the start of its turn), and the impulse from one that
/// comes after it by the travel of the next tick. Every other animal stays in the parallel batch systems.
/// What an island of the ordered pass asks the world for (applied after the islands ran, in the order of the islands).
enum Effect {
    Mark(Entity),
    Unmark(Entity),
    Moving(Entity),
    Crowded(Entity),
    Fall(Entity, f32),
}
type OrderedItems = (
    Entity,
    &'static mut Pos,
    &'static mut Vel,
    &'static Yaw,
    &'static mut Contact,
    &'static mut Drive,
    &'static Body,
    &'static mut Bounds,
    &'static FluidState,
    Has<Following>,
    Option<&'static crate::goals::Jump>,
    &'static FluidParams,
    Has<Moving>,
    Option<&'static Immune>,
);
/// One island of the ordered pass, its animals in the order of creation: each travels, then pushes every animal that
/// overlaps its box now (see `ordered_pass`). Touches only the components of the animals of its island and of the
/// animals its members overlap, which no other island touches.
#[allow(clippy::type_complexity)]
fn run_island(
    ids: &[Entity],
    level: &Level,
    table: &SpeciesTable,
    idx: &Index,
    q: &Query<OrderedItems>,
    marked: &Query<Entity, With<Overlapping>>,
    moving_now: &Query<(), With<Moving>>,
) -> Vec<Effect> {
    let mut out = Vec::new();
    let mut near: Vec<Entity> = Vec::new();
    let mut hits: Vec<(Entity, f64, f64)> = Vec::new();
    let ro = q.as_readonly();
    for &e in ids {
        {
            // SAFETY: the islands are disjoint: no other thread touches the components of this animal.
            let Ok((_, mut p, mut v, y, mut c, mut d, body, mut b, fs, _following, jump, sp, _, imm)) = (unsafe { q.get_unchecked(e) }) else { continue };
            let mut fall = 0.0_f32;
            let jumping = jump.is_some_and(|j| j.0);
            travel_one(level, sp, &mut p, &mut v, y, &mut c, &mut d, body, &mut b, fs, jumping, &mut fall);
            if fall > 0.0 && !imm.is_some_and(|i| i.has(IMMUNE_FALL)) {
                out.push(Effect::Fall(e, fall));
            }
        }
        let Ok(me) = ro.get(e) else { continue };
        let b = me.7.0;
        let still = me.2.0.x.abs() + me.2.0.z.abs() < 1.0e-4;
        let (mn, mx) = (b.min_position(), b.max_position());
        let (cx, cz) = ((mn.x + mx.x) / 2.0, (mn.z + mx.z) / 2.0);
        near.clear();
        hits.clear();
        idx.for_near(cx, cz, |o| near.push(o));
        let mut own = (0.0_f64, 0.0_f64);
        let mut overlapping = 0;
        let mut close = 0;
        let wide = b.inflate(Vec3::new(ORDERED_MARGIN, 0.0, ORDERED_MARGIN));
        for &o in &near {
            if o == e {
                continue;
            }
            let Ok(oth) = ro.get(o) else { continue };
            let ob = oth.7;
            if !b.intersects(ob.0) {
                if !still && wide.intersects(ob.0) {
                    close += 1;
                    if !marked.contains(o) {
                        out.push(Effect::Mark(o));
                    }
                }
                continue;
            }
            overlapping += 1;
            let (omn, omx) = (ob.0.min_position(), ob.0.max_position());
            let (mut xa, mut za) = ((omn.x + omx.x) / 2.0 - cx, (omn.z + omx.z) / 2.0 - cz);
            let mut dd = xa.abs().max(za.abs());
            if dd < f64::from(0.01_f32) {
                continue;
            }
            dd = dd.sqrt();
            xa /= dd;
            za /= dd;
            let pow = (1.0_f64 / dd).min(1.0);
            xa *= pow * f64::from(0.05_f32);
            za *= pow * f64::from(0.05_f32);
            own.0 -= xa;
            own.1 -= za;
            hits.push((o, xa, za));
        }
        // SAFETY: as above (this animal and the ones it overlaps belong to this island).
        if let Ok((_, _, mut v, ..)) = unsafe { q.get_unchecked(e) } {
            v.0 = Vec3::new(v.0.x + own.0, v.0.y, v.0.z + own.1);
        }
        for &(o, xa, za) in &hits {
            // SAFETY: as above.
            if let Ok((_, _, mut ov, ..)) = unsafe { q.get_unchecked(o) } {
                ov.0 = Vec3::new(ov.0.x + xa, ov.0.y, ov.0.z + za);
            }
            if !marked.contains(o) {
                out.push(Effect::Mark(o));
            }
            if !moving_now.contains(o) {
                out.push(Effect::Moving(o));
            }
        }
        if overlapping + close == 0 {
            out.push(Effect::Unmark(e));
        } else if overlapping > 0 && overlapping + 1 >= MAX_ENTITY_CRAMMING {
            out.push(Effect::Crowded(e));
        }
    }
    out
}
fn find(parent: &mut [u32], mut i: u32) -> u32 {
    while parent[i as usize] != i {
        parent[i as usize] = parent[parent[i as usize] as usize];
        i = parent[i as usize];
    }
    i
}
/// The ordered push mode (`Config.ordered_push`): the animals that overlap another travel and push in islands: animals
/// connected by overlap (or by a gap too small to cross in one tick), each island in the order of creation of its
/// members, with the `Mob` pass's semantics: an animal travels, then pushes every animal that overlaps it (its own
/// velocity and the other's at once); the impulse an animal gets from one that comes before it is used by its own
/// travel of this tick, from one that comes after it by the next tick's. The islands do not touch each other, so they run
/// one after the other or in parallel (`Config.parallel`) with the same result. Every other animal stays in the batch.
#[allow(clippy::type_complexity)]
pub(crate) fn ordered_pass(
    cfg: Res<Config>,
    level: Res<Level>,
    table: Res<SpeciesTable>,
    idx: Res<Index>,
    mut stats: ResMut<OrderedStats>,
    mut queue: ResMut<DamageQueue>,
    q: Query<OrderedItems>,
    marked: Query<Entity, With<Overlapping>>,
    uids: Query<&crate::goals::Uid>,
    moving_now: Query<(), With<Moving>>,
    mut commands: Commands,
    mut node: Local<Vec<u32>>,
) {
    if !cfg.ordered_push {
        return;
    }
    let mut ids: Vec<Entity> = marked.iter().collect();
    // The order of creation (the `Mob` herd's list order): the animal's `Uid` (an entity index is reused after a death).
    ids.sort_unstable_by_key(|e| uids.get(*e).map_or(0, |u| u.0));
    stats.last = ids.len();
    stats.total += ids.len() as u64;
    stats.ticks += 1;
    // The islands: union-find over the marked animals and the animals within one tick's reach of them.
    let ro = q.as_readonly();
    // `node[entity index]`: the node of the animal in the union-find (u32::MAX: none); only the touched slots are reset.
    let mut touched: Vec<u32> = Vec::new();
    let need = ids.iter().map(|e| e.index().index() as usize + 1).max().unwrap_or(0);
    if node.len() < need {
        node.resize(need, u32::MAX);
    }
    let mut parent: Vec<u32> = Vec::with_capacity(ids.len() * 2);
    let mut ents: Vec<Entity> = Vec::with_capacity(ids.len() * 2);
    for &e in &ids {
        node[e.index().index() as usize] = parent.len() as u32;
        touched.push(e.index().index());
        parent.push(parent.len() as u32);
        ents.push(e);
    }
    let reach_by = island_reach();
    let mut near: Vec<Entity> = Vec::new();
    for &e in &ids {
        let Ok(me) = ro.get(e) else { continue };
        let reach = me.7.0.inflate(Vec3::new(reach_by, 0.0, reach_by));
        let (mn, mx) = (me.7.0.min_position(), me.7.0.max_position());
        near.clear();
        idx.for_near((mn.x + mx.x) / 2.0, (mn.z + mx.z) / 2.0, |o| near.push(o));
        let ne = node[e.index().index() as usize];
        for &o in &near {
            if o == e || !ro.get(o).is_ok_and(|ob| reach.intersects(ob.7.0)) {
                continue;
            }
            // An animal that is not in the pass is a node too (it receives impulses: two islands must not both give it any).
            let slot = o.index().index() as usize;
            if slot >= node.len() {
                node.resize(slot + 1, u32::MAX);
            }
            if node[slot] == u32::MAX {
                node[slot] = parent.len() as u32;
                touched.push(o.index().index());
                parent.push(parent.len() as u32);
                ents.push(o);
            }
            let (ra, rb) = (find(&mut parent, ne), find(&mut parent, node[slot]));
            if ra != rb {
                parent[ra.max(rb) as usize] = ra.min(rb);
            }
        }
    }
    let mut islands: Vec<Vec<Entity>> = Vec::new();
    let mut root_island: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
    for &e in &ids {
        let r = find(&mut parent, node[e.index().index() as usize]);
        let i = *root_island.entry(r).or_insert_with(|| {
            islands.push(Vec::new());
            islands.len() - 1
        });
        islands[i].push(e);
    }
    for i in touched {
        node[i as usize] = u32::MAX;
    }
    let _ = ents;

    stats.islands += islands.len() as u64;
    stats.largest = stats.largest.max(islands.iter().map(Vec::len).max().unwrap_or(0));
    for i in &islands {
        let bucket = match i.len() {
            1 => 0,
            2..=3 => 1,
            4..=10 => 2,
            11..=100 => 3,
            _ => 4,
        };
        stats.sizes[bucket] += 1;
    }
    let effects: Vec<Vec<Effect>> = if cfg.parallel && islands.len() > 1 {
        let pool = bevy_tasks::ComputeTaskPool::get();
        let (level, table, idx, q, marked, moving_now) = (&*level, &*table, &*idx, &q, &marked, &moving_now);
        pool.scope(|s| {
            for i in &islands {
                s.spawn(async move { run_island(i, level, table, idx, q, marked, moving_now) });
            }
        })
    } else {
        islands.iter().map(|i| run_island(i, &level, &table, &idx, &q, &marked, &moving_now)).collect()
    };
    for fx in effects {
        for f in fx {
            match f {
                Effect::Mark(o) => {
                    commands.entity(o).insert(Overlapping);
                }
                Effect::Unmark(e) => {
                    commands.entity(e).remove::<Overlapping>();
                }
                Effect::Moving(o) => {
                    commands.entity(o).insert(Moving);
                }
                Effect::Crowded(e) => {
                    commands.entity(e).insert(Crowded);
                }
                Effect::Fall(e, f) => queue.add(e, f, Cause::Fall),
            }
        }
    }
}
/// `gamerule maxEntityCramming`: 24 others overlapping hurt an animal.
pub const MAX_ENTITY_CRAMMING: usize = 24;
/// Overlapping at least `MAX_ENTITY_CRAMMING - 1` others when last looked at: the cramming module visits it each tick
/// (`LivingEntity.pushEntities` counts the overlapping entities of every entity, every tick) until fewer overlap it.
#[derive(Component, Default)]
#[component(storage = "SparseSet")]
pub struct Crowded;
/// `LivingEntity.pushEntities`'s cramming: with `maxEntityCramming` or more others overlapping, one tick in four hurts 6.
/// The entities are taken one after the other and one that this hit kills no longer counts for the next (as the `Mob`
/// pass sees the deaths of the entities ticked before it): a pile of 25 loses one animal, not six, when the hurt windows end.
#[allow(clippy::type_complexity)]
fn cramming(
    clock: Res<Clock>,
    idx: Res<Index>,
    boxes: Query<&Bounds, Allow<Disabled>>,
    mut q: Query<(Entity, &Bounds, &mut Rng, &Health, &HurtAt), With<Crowded>>,
    mut queue: ResMut<DamageQueue>,
    mut commands: Commands,
) {
    let mut dying: Vec<Entity> = Vec::new();
    for (e, b, mut rng, health, hurt) in &mut q {
        let (mn, mx) = (b.0.min_position(), b.0.max_position());
        let mut n = 0;
        idx.for_near((mn.x + mx.x) / 2.0, (mn.z + mx.z) / 2.0, |o| {
            if o != e && !dying.contains(&o) && boxes.get(o).is_ok_and(|ob| b.0.intersects(ob.0)) {
                n += 1;
            }
        });
        if n < MAX_ENTITY_CRAMMING {
            commands.entity(e).remove::<Crowded>();
        } else if rng.next_int(4) == 0 {
            queue.add(e, 6.0, Cause::Cramming);
            // The hit is applied at the start of the next tick.
            let in_window = clock.of(e) + 1 < hurt.at + 10 && hurt.at != 0;
            let applied = if in_window { (6.0 - hurt.last).max(0.0) } else { 6.0 };
            if health.0 - applied <= 0.0 {
                dying.push(e);
            }
        }
    }
}
/// The half of a push a cow that did not move is owed: its velocity changes, its physics is active.
/// Adds an impulse to the neighbour `o`'s pending push: the impulses of all its neighbours are SUMMED (`Entity.push` adds to the velocity), not
/// overwritten by the last one, so the total does not depend on the order the pairs were visited in.
fn add_push(w: &mut World, o: Entity, push: Vec3, wake: bool) {
    let Ok(mut em) = w.get_entity_mut(o) else { return };
    match em.get_mut::<PushIn>() {
        Some(mut p) => p.0 = Vec3::new(p.0.x + push.x, p.0.y + push.y, p.0.z + push.z),
        None => {
            em.insert(PushIn(push));
        }
    }
    if wake {
        em.insert(Moving);
    }
}
#[allow(clippy::type_complexity)]
fn apply_pushes(
    mut q: Query<(Entity, &PushIn, &mut Vel)>,
    mut commands: Commands,
    mut on: ResMut<crate::goals::Online>,
    mounts: Query<(Entity, &crate::mount::Mount, &Yaw, &Body, &Pos), With<crate::mount::Ridden>>,
    mut riders: Query<(&mut Pos, Option<&mut Yaw>), (With<crate::mount::Riding>, Without<crate::mount::Ridden>)>,
) {
    for (e, p, mut v) in &mut q {
        v.0 = Vec3::new(v.0.x + p.0.x, v.0.y, v.0.z + p.0.z);
        commands.entity(e).remove::<PushIn>();
    }
    // The seats: no system of their own, so no cost when nothing is ridden (the queries are sparse sets).
    crate::mount::seats(&mounts, &mut riders, &mut on, &mut commands);
}

/// What is due at this tick in the timer wheel: a stroll attempt (a spot is picked, or the attempt fails and is
/// armed again, or the goal is refused for lack of a player and is not re-armed), a baby grows up.
#[allow(clippy::type_complexity)]
pub(crate) fn due(
    mut clock: ResMut<Clock>,
    mut wheel: ResMut<Wheel>,
    mut fired: ResMut<crate::goals::Fired>,
    mut grown: ResMut<crate::farm::Grown>,
    mut events: ResMut<crate::goals::Events>,
    mut players: ResMut<crate::hostile::PlayerState>,
    mut sun: ResMut<crate::hostile::SunFired>,
    mut babies: Query<(&mut Body, &Sizes, &GrowAt, &Pos, &mut Bounds)>,
    burners: Query<(Entity, &Burning, Option<&Immune>)>,
    mut queue: ResMut<DamageQueue>,
    mut commands: Commands,
) {
    clock.0 += 1;
    events.swap();
    wheel.advance(clock.0);
    for id in std::mem::take(&mut players.gaze_dirty) {
        events.gaze_changed.push(id);
    }
    let slot = (clock.0 % WHEEL as u64) as usize;
    let list = std::mem::take(&mut wheel.slots[slot]);
    if wheel.track {
        for &(e, d) in &list {
            wheel.fired(e, clock.0, d);
        }
    }
    for (e, d) in list {
        match d {
            // The goal selector (`goals.rs`) owns the stroll attempt, like every other goal's deadline.
            Due::Stroll => fired.0.push((e, crate::behaviours::stroll::FIRED)),
            Due::Goal(k) => fired.0.push((e, k)),
            Due::Egg => commands.queue(move |w: &mut World| crate::farm::lay_deadline(w, e)),
            Due::Expire => commands.queue(move |w: &mut World| crate::projectile::expire(w, e)),
            Due::Sensor(i) => commands.queue(move |w: &mut World| crate::brain::sensor_due(w, e, i)),
            Due::ItemExpire(t) => commands.queue(move |w: &mut World| crate::item::expire_due(w, e, t)),
            Due::Hop => commands.queue(move |w: &mut World| crate::locomotion::hop::due(w, e)),
            Due::OrbExpire(t) => commands.queue(move |w: &mut World| crate::orb::expire_due(w, e, t)),
            Due::OrbCheck(t) => commands.queue(move |w: &mut World| crate::orb::check_due(w, e, t)),
            Due::ItemCheck(t) => commands.queue(move |w: &mut World| crate::item::check_due(w, e, t)),
            Due::BrainWake => commands.queue(move |w: &mut World| crate::brain::wake(w, e)),
            Due::Call(i) => commands.queue(move |w: &mut World| {
                let f = w.resource::<Callbacks>().0[usize::from(i)];
                f(w, e);
            }),
            Due::Forget(l, t) => commands.queue(move |w: &mut World| crate::brain::forget_due(w, e, l, t)),
            Due::Sun => sun.0.push(e),
            Due::Effect => commands.queue(move |w: &mut World| crate::effects::due(w, e)),
            Due::Raids => commands.queue(|w: &mut World| crate::raids::raid_due(w)),
            Due::Fuse => commands.queue(move |w: &mut World| crate::explosion::fuse_due(w, e)),
            Due::Despawn => commands.queue(move |w: &mut World| crate::despawn::due(w, e)),
            Due::Burn => {
                if let Ok((e2, burning, imm)) = burners.get(e) {
                    let now = clock.of(e);
                    if now < burning.until {
                        if !imm.is_some_and(|i| i.has(crate::effects::IMMUNE_FIRE)) {
                            queue.add(e2, 1.0, Cause::OnFire);
                        }
                        wheel.push(clock.at(e, now + 20), e, Due::Burn);
                    } else {
                        commands.entity(e).remove::<Burning>();
                        events.extinguished.push(e);
                    }
                }
            }
            Due::Grow => {
                if let Ok((mut b, sizes, g, p, mut bb)) = babies.get_mut(e) {
                    let now = clock.of(e);
                    if g.0 > now {
                        // A wait longer than the wheel is put back (a calf grows in 24 000 ticks).
                        wheel.push(clock.at(e, g.0), e, Due::Grow);
                    } else if g.0 == now {
                        b.width = sizes.adult.0;
                        b.height = sizes.adult.1;
                        bb.0 = box_at(p.0, sizes.adult.0, sizes.adult.1);
                        commands.entity(e).remove::<GrowAt>().insert(Recheck);
                        grown.0.push(e);
                        events.grew.push(e);
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
        let mut herd = Self::build(n, spacing, sleep, parallel, seed, hazards, other_every, None, false);
        // The loot tables are read once, now: the first death of a bench would read them in the middle of a measure.
        crate::loot::preload(&mut herd.world);
        herd
    }
    /// An empty walled slab of `half` blocks each way.
    pub fn with_slab(half: i32, sleep: bool, hazards: bool) -> Self {
        Self::build(0, 1.0, sleep, false, 5, hazards, 0, Some(half), false)
    }
    /// Day or night for the spawn rules (`MobWorld.daylight`: the sky darkens by 11 at night); the world is the same one, read again.
    pub fn set_daylight(&mut self, day: bool) {
        let mut level = *self.world.resource::<Level>();
        let w: &'static MobWorld<'static> = Box::leak(Box::new(MobWorld { daylight: day, ..level.mw.0.clone() }));
        level.mw = Mw(w);
        *self.world.resource_mut::<Level>() = level;
    }
    /// Turns the natural spawning on, with the spawn lists of `biome` (the game's data) and the seed of its random.
    pub fn enable_spawning(&mut self, biome: &str, seed: i64) {
        let mw = self.world.resource::<Level>().mw.0;
        let info = mw.biomes.get(*mw.biomes.index.get(biome).expect("a known biome"));
        let table = crate::spawning::Table::from_biome(info, &species_table());
        crate::spawning::ensure(&mut self.world);
        let mut sp = crate::spawning::Spawner::new(table, seed);
        sp.biome = biome.to_owned();
        self.world.insert_resource(sp);
    }
    /// An empty slab of `half` blocks each way with a real surface (grass on dirt on stone down to the bottom of the world) for the
    /// natural spawning (`spawning.rs`): the surface is the block y = 0, the creatures stand at y = 1.
    pub fn with_spawn_world_kind(half: i32, sleep: bool, kind: crate::spawning::Kind) -> Self {
        // The kind is read by the builder: no other world is built meanwhile.
        static BUILDING: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = BUILDING.lock().unwrap_or_else(|e| e.into_inner());
        crate::spawning::set_kind(kind);
        let mut h = Self::build(0, 1.0, sleep, false, 5, false, 0, Some(half), true);
        crate::loot::preload(&mut h.world);
        h
    }
    pub fn with_spawn_world(half: i32, sleep: bool) -> Self {
        Self::with_spawn_world_kind(half, sleep, crate::spawning::Kind::Plains)
    }
    #[allow(clippy::too_many_arguments)]
    fn build(n: usize, spacing: f64, sleep: bool, parallel: bool, seed: u64, hazards: bool, other_every: usize, slab: Option<i32>, spawn_world: bool) -> Self {
        let side = (n as f64).sqrt().ceil() as i32;
        let half = slab.unwrap_or(((f64::from(side) * spacing / 2.0) as i32 + 6).max(20));
        let mut world = World::new();
        crate::goals::reset_uids();
        let (level, species, _kind) = if spawn_world { build_level_with(half, crate::spawning::world_builder(half)) } else { build_level(half, hazards) };
        world.insert_resource(Hazards::build(&level));
        world.insert_resource(DamageQueue::default());
        world.insert_resource(HurtCauses::default());
        world.insert_resource(BlockChanges::default());
        world.insert_resource(StateChanges::default());
        world.insert_resource(level);
        let _ = species;
        world.insert_resource(species_table());
        world.insert_resource(Config { sleep, parallel, ordered_push: false, keep_touching: std::env::var_os("TOUCHING").is_some(), path_budget: std::env::var("PATH_BUDGET").ok().and_then(|v| v.parse().ok()).unwrap_or(0), push_form: if std::env::var("PUSH_FORM").as_deref() == Ok("b") { 1 } else { 0 }, push_k: std::env::var("PUSH_K").ok().and_then(|v| v.parse().ok()).unwrap_or(1.0) });
        world.insert_resource(OrderedStats::default());
        world.insert_resource(Clock::default());
        world.insert_resource(Players::default());
        world.insert_resource(Wheel::default());
        crate::goals::insert_resources(&mut world);
        crate::hostile::insert_resources(&mut world);
        world.insert_resource(Index::new(-f64::from(half), (f64::from(2 * half) / CELL) as usize + 2));
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
            self.world.get_mut::<Stroll>(e).unwrap().scheduled = true;
        }
        crate::goals::arm_all(&mut self.world);
        self.world.run_system_cached(reindex).unwrap();
        out
    }
    /// Micro-bench of the path search, 1: every animal gets a target 1.5 blocks away and `NeedsPath`.
    pub fn bench_pathfind_prepare(&mut self) {
        let ids: Vec<Entity> = self.world.query_filtered::<Entity, With<Stroll>>().iter(&self.world).collect();
        for &e in &ids {
            let p = self.world.get::<Pos>(e).unwrap().0;
            let mut s = self.world.get_mut::<Stroll>(e).unwrap();
            (s.tx, s.tz, s.ty) = (p.x + 1.5, p.z, p.y.floor() as i32);
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
            let target = [s.tx.floor() as i32, s.ty, s.tz.floor() as i32];
            let max_len = sp.follow_range;
            std::hint::black_box(Finder::new(mw, &pm, Evaluator::Walk).find(&[target], max_len, reach, (max_len * 16.0).floor() as i32));
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

#[cfg(test)]
mod tests {
    /// A deadline of ANY length lands on its exact tick: the wheel keeps what is beyond its horizon and brings it in by itself.
    #[test]
    fn a_wait_of_ten_thousand_ticks_falls_at_the_exact_tick() {
        let mut w = Wheel::default();
        let e = Entity::from_raw_u32(7).unwrap();
        for at in [5_u64, 4095, 4096, 4097, 8192, 10_000, 20_000] {
            w.push(at, e, Due::Burn);
        }
        let mut fired = Vec::new();
        for t in 1..=20_001_u64 {
            w.advance(t);
            let n = w.due_len(t);
            for _ in 0..n {
                fired.push(t);
            }
            w.slots[(t % WHEEL as u64) as usize].clear();
        }
        assert_eq!(fired, vec![5, 4095, 4096, 4097, 8192, 10_000, 20_000]);
        assert!(w.far.is_empty());
    }
    use super::*;

    /// Two cows standing in each other are pushed apart; a sleeping cow they overlap is woken; a baby grows up.
    #[test]
    fn cows_push_each_other_and_babies_grow() {
    if crate::cow::skip_unless_small_body("cows_push_each_other_and_babies_grow") {
        return;
    }
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
    if crate::cow::skip_unless_small_body("falls_fire_lava_snow_and_cactus_hurt_a_cow") {
        return;
    }
    if crate::cow::skip_unless_walker("falls_fire_lava_snow_and_cactus_hurt_a_cow") {
        return;
    }
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
        if species().rules.iter().any(|r| r.rule.name == "wing_flap") {
            // A hen takes no fall damage (`causeFallDamage` answers no).
            assert!(health(&herd, hurt) == Some(species().max_health) && health(&herd, dies) == Some(species().max_health), "{:?} {:?}", health(&herd, hurt), health(&herd, dies));
        } else {
            assert!(health(&herd, hurt).is_some_and(|h| h > 0.0 && h < species().max_health), "{:?}", health(&herd, hurt));
            assert!(health(&herd, dies).is_none(), "the cow that fell 39 blocks lives");
        }
        assert!(health(&herd, cactus).is_none_or(|h| h < species().max_health), "the cactus did not hurt");
        for _ in 0..100 {
            herd.tick();
        }
        show(&herd, 140);
        assert!(health(&herd, lava).is_none(), "the cow in lava lives: {:?}", health(&herd, lava));
        assert!(health(&herd, fire).is_none_or(|h| h <= species().max_health - 4.0), "the fire: {:?}", health(&herd, fire));
        for _ in 0..200 {
            herd.tick();
        }
        show(&herd, 340);
        assert!(health(&herd, snow).is_none_or(|h| h < species().max_health), "the powder snow: {:?}", health(&herd, snow));
        // Still burning or dead at tick 340: a species with the panic goal flees the fire burning; one without wanders
        // out of it by chance, and the fire ends (160 ticks) before the tick 340.
        if species().goals.has(&crate::behaviours::panic::MODULE) {
            assert!(herd.world.get::<Burning>(fire).is_some() || health(&herd, fire).is_none());
        }
        // The pond is 2 blocks deep: only an animal whose eyes sit lower than 1.4 blocks goes under (a zombie's are at 1.74).
        // (A hen's glide cuts her sinking speed in water too; whether she drowns as a `Mob` hen does was not compared.)
        // (An amphibian or an aquatic animal does not drown, as in the `Mob`: `breathes_water`, the frog, the axolotl, the fish.)
        if species().eye_height <= 1.4 && !species().breathes_water && !species().rules.iter().any(|r| r.rule.name == "wing_flap") {
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
        use crate::goals::{carrot, Online, Panic, Player, Tempting, wheat, feed};
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
            assert_eq!(herd.world.get::<Panic>(e).unwrap().speed, sp.goals.param::<crate::behaviours::panic::Params>(&crate::behaviours::panic::MODULE, 0).unwrap().speed);
        }
        // A player with a carrot in the middle: the pigs are tempted, the cows are not; then with wheat, the reverse.
        let tempted = |herd: &mut Herd, id: u8| {
            all.iter().filter(|a| a.1 == id && herd.world.get::<Tempting>(a.0).is_some()).count()
        };
        herd.world.resource_mut::<Online>().set(Player { id: 1, x: 0.0, y: 1.0, z: 0.0, main: carrot(), off: 0, ..Default::default() });
        for _ in 0..8 {
            herd.tick();
        }
        let (cows_c, pigs_c) = (tempted(&mut herd, 0), tempted(&mut herd, 1));
        herd.world.resource_mut::<Online>().set(Player { id: 1, x: 0.0, y: 1.0, z: 0.0, main: wheat(), off: 0, ..Default::default() });
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
    if crate::cow::skip_unless_small_body("block_changes_wake_only_what_they_touch") {
        return;
    }
    if crate::cow::skip_unless_walker("block_changes_wake_only_what_they_touch") {
        return;
    }
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
        // The fall of a block: 6 ticks, more for a hen (her glide cuts the fall speed).
        for _ in 0..60 {
            herd.tick();
            if !moving(&herd, a) {
                break;
            }
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
    if crate::cow::skip_unless_small_body("water_placed_floats_extinguishes_and_works_at_height") {
        return;
    }
    if crate::cow::skip_unless_walker("water_placed_floats_extinguishes_and_works_at_height") {
        return;
    }
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
        if species().goals.has(&crate::behaviours::float::MODULE) {
            assert!(y > 1.5, "the cow floats up: y = {y}");
        }
        // Fire in the cell of the second cow, then water over it: the burning stops.
        // (The burner stands still for this: a panic that runs from the fire is the goals' business, not the hazards'.)
        herd.world.entity_mut(burner).remove::<crate::goals::Panic>();
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
    if crate::cow::skip_unless_small_body("a_hit_knocks_a_cow_back") {
        return;
    }
    if crate::cow::skip_unless_walker("a_hit_knocks_a_cow_back") {
        return;
    }
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
    if crate::cow::skip_unless_small_body("the_cows_walk_sleep_and_wake_on_the_slab") {
        return;
    }
    if crate::cow::skip_unless_farm_goals("the_cows_walk_sleep_and_wake_on_the_slab") {
        return;
    }
    if crate::cow::skip_unless_walker("the_cows_walk_sleep_and_wake_on_the_slab") {
        return;
    }
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
        // The wall of the slab holds the walkers. An animal that leaps (the goat up to 5 blocks, the frog 2) can go over it, as in the `Mob` (a leap
        // lands on any block with a solid one below it); what it may not do is stay under the world: there it is hurt 4 a tick and dies
        // (`Cause::OutOfWorld`), so no creature is left below it. The other species stay on the slab, on its ground.
        let void_y = f64::from(herd.world.resource::<Level>().mw.0.world.bottom_y() - 64);
        let rows: Vec<(Vec3, bool)> = herd.world.query::<(&Pos, Has<crate::behaviours::long_jump::LongJumpState>)>().iter(&herd.world).map(|(p, l)| (p.0, l)).collect();
        for (p, leaps) in rows {
            if leaps {
                assert!(p.x.is_finite() && p.y.is_finite() && p.z.is_finite() && p.y >= void_y, "a leaper under the world is still there: {p:?}");
            } else {
                assert!(p.x.abs() < half && p.z.abs() < half && p.y > 0.99 && p.y < 1.2, "{p:?}");
            }
        }
        assert!(travelled > 100, "only {travelled} cows travelled more than 3 blocks");
        assert!(max_asleep > 30 && min_asleep < 300, "{min_asleep}..{max_asleep} asleep of 300");
        // The player leaves: after 100 ticks every cow is idle and asleep, for good.
        herd.set_players(vec![]);
        for _ in 0..400 {
            herd.tick();
        }
        // On the slab (a leaper that went over the wall is where its leap took it, not on the ground of the test: it is left out of both counts).
        let on_slab = |herd: &mut Herd| -> (usize, usize) {
            let rows: Vec<(Vec3, bool, bool)> = herd.world.query::<(&Pos, Has<Moving>, Has<Following>)>().iter(&herd.world).map(|(p, m, f)| (p.0, m, f)).collect();
            let here: Vec<&(Vec3, bool, bool)> = rows.iter().filter(|(p, _, _)| p.x.abs() < half && p.z.abs() < half && p.y > 0.99 && p.y < 1.2).collect();
            (here.iter().filter(|(_, m, f)| !m && !f).count(), here.len())
        };
        let (idle, here) = on_slab(&mut herd);
        assert!(idle * 300 >= here * 295, "{idle} of {here} on the slab asleep after the player left");
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


    /// A cow that walks out of the 32 blocks of a player that stays where it is stops being active when it changes
    /// cell (the player moved nowhere: only the cell change tells).
    #[test]
    fn a_cow_leaving_a_standing_player_stops_being_active() {
        if skip_unless_walker("a_cow_leaving_a_standing_player_stops_being_active") {
            return;
        }
        if std::env::var_os("HORIZON_TEST_RESOURCES").is_none() {
            return;
        }
        let mut herd = Herd::new(0, 6.0, true, false, 3);
        let e = herd.world.spawn(cow_bundle(0.5, 0.5, 11)).id();
        herd.set_players(vec![(0.5, 0.5)]);
        herd.tick();
        herd.tick();
        assert_eq!(herd.world.get::<ActiveUntil>(e).unwrap().0, u64::MAX, "a cow next to a player is active");
        herd.teleport(e, Vec3::new(40.5, 1.0, 0.5));
        herd.tick();
        herd.tick();
        let until = herd.world.get::<ActiveUntil>(e).unwrap().0;
        assert!(until != u64::MAX && until <= herd.world.resource::<Clock>().0 + 100, "still active without end: {until}");
    }
}

/// Puts the entity at `at`: its box follows, and the events that can put it into blocks or a fluid are raised.
pub fn teleport(world: &mut World, e: Entity, at: Vec3) {
    let b = *world.get::<Body>(e).unwrap();
    world.get_mut::<Pos>(e).unwrap().0 = at;
    world.get_mut::<Vel>(e).unwrap().0 = Vec3::ZERO;
    world.get_mut::<Bounds>(e).unwrap().0 = box_at(at, b.width, b.height);
    world.entity_mut(e).insert((Recheck, Moving));
}

/// The hook of `Moving`: an item or an orb that starts to move is listed for its system (nothing for the others).
pub fn moving_added(mut world: bevy_ecs::world::DeferredWorld, ctx: bevy_ecs::lifecycle::HookContext) {
    if world.get::<crate::item::Item>(ctx.entity).is_some() {
        world.commands().entity(ctx.entity).insert(crate::item::ItemAwake);
    } else if world.get::<crate::orb::Orb>(ctx.entity).is_some() {
        world.commands().entity(ctx.entity).insert(crate::orb::OrbAwake);
    }
}

#[cfg(test)]
mod index_tests {
    use super::*;
    /// The two-level `for_radius` visits the same entities in the same order as a loop over every column of the box.
    #[test]
    fn for_radius_matches_column_loop() {
        let mut world = World::new();
        let width = 70;
        let mut idx = Index::new(0.0, width);
        let mut s = 12345u64;
        let mut rnd = move |n: u64| {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            s % n
        };
        let mut placed = Vec::new();
        for _ in 0..400 {
            let e = world.spawn_empty().id();
            let c = (rnd((width * width) as u64)) as u32;
            idx.insert(e, c);
            placed.push((e, c));
        }
        for k in 0..60 {
            let (e, c) = placed.swap_remove(rnd(placed.len() as u64) as usize);
            idx.remove(e, c);
            if k % 2 == 0 {
                let c2 = rnd((width * width) as u64) as u32;
                idx.insert(e, c2);
                placed.push((e, c2));
            }
        }
        for _ in 0..300 {
            let (x, z, r) = (rnd(90) as f64 - 10.0, rnd(90) as f64 - 10.0, rnd(40) as f64);
            let mut got = Vec::new();
            idx.for_radius(x, z, r, |e| got.push(e));
            let w = width as i64;
            let mut want = Vec::new();
            for zz in idx.column(z - r - 1.0).max(0)..=idx.column(z + r + 1.0).min(w - 1) {
                for xx in idx.column(x - r - 1.0).max(0)..=idx.column(x + r + 1.0).min(w - 1) {
                    want.extend_from_slice(&idx.cells[(zz * w + xx) as usize]);
                }
            }
            assert_eq!(got, want, "x {x} z {z} r {r}");
        }
    }
}

/// The hooks that register the carriers of a watcher component with the `Index`: `on_insert = watch_in::<K>, on_discard = watch_out::<K>`.
pub(crate) fn watch_in<const K: usize>(mut world: bevy_ecs::world::DeferredWorld, ctx: bevy_ecs::lifecycle::HookContext) {
    let cell = world.get::<Cell>(ctx.entity).map_or(u32::MAX, |c| c.0);
    if let Some(mut idx) = world.get_resource_mut::<Index>() {
        idx.watch_add(ctx.entity, K, cell);
    }
}
pub(crate) fn watch_out<const K: usize>(mut world: bevy_ecs::world::DeferredWorld, ctx: bevy_ecs::lifecycle::HookContext) {
    let cell = world.get::<Cell>(ctx.entity).map_or(u32::MAX, |c| c.0);
    if let Some(mut idx) = world.get_resource_mut::<Index>() {
        idx.watch_remove(ctx.entity, K, cell);
    }
}

pub use crate::cow_helpers::*;
