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
/// `RandomStrollGoal`: the wait before the next attempt, whether a walk is on, where to.
#[derive(Component, Clone, Copy, Debug)]
pub struct Stroll {
    pub wait: i32,
    pub walking: bool,
    /// A spot was picked and its path is not made yet.
    pub pending: bool,
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
}
impl Default for Nav {
    fn default() -> Self {
        Self { path: None, last_check: 0, last_pos: Vec3::ZERO }
    }
}
/// The size of an adult and of a baby of the species (data, not code).
#[derive(Component, Clone, Copy, Debug)]
pub struct Sizes {
    pub adult: (f32, f32),
    pub baby: (f32, f32),
}
/// On the ground, motionless, no walk, a stroll wait still to run: no movement system visits it.
#[derive(Component, Clone, Copy, Debug)]
#[cfg_attr(feature = "sparse", component(storage = "SparseSet"))]
pub struct Asleep {
    /// The tick at which the stroll wait ends and the cow wakes (it is in the timer wheel at that tick).
    pub until: u64,
}
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
/// What a woken sleeper still owes its stroll wait.
#[derive(Component, Clone, Copy, Debug)]
pub struct Resume(pub i32);
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
    Wake { until: u64 },
    Grow,
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
/// One cow of the spatial index: its box and whether it sleeps.
#[derive(Clone, Copy, Debug)]
pub struct Entry {
    pub entity: Entity,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub half: f64,
    pub height: f64,
    pub asleep: bool,
    pub until: u64,
}
/// The cows by 2-block column, as they were at the start of the step (rebuilt by `index`): the neighbours a
/// cow reads are never half-updated, whatever the order the cows are visited in.
#[derive(Resource, Default)]
pub struct Grid {
    pub origin: f64,
    pub width: usize,
    start: Vec<u32>,
    items: Vec<Entry>,
    cell_of: Vec<u32>,
    next: Vec<u32>,
    scratch: Vec<Entry>,
    stamp: Vec<u32>,
}
const CELL: f64 = 2.0;
impl Grid {
    fn column(&self, x: f64) -> i64 {
        ((x - self.origin) / CELL).floor() as i64
    }
    fn cell(&self, x: f64, z: f64) -> usize {
        let cx = self.column(x).clamp(0, self.width as i64 - 1) as usize;
        let cz = self.column(z).clamp(0, self.width as i64 - 1) as usize;
        cz * self.width + cx
    }
    /// A counting sort of the entries by column, in buffers that are kept from one step to the next.
    pub fn rebuild(&mut self, entries: impl Iterator<Item = Entry>) {
        let cells = self.width * self.width;
        self.start.clear();
        self.start.resize(cells + 1, 0);
        let mut all = std::mem::take(&mut self.scratch);
        all.clear();
        all.extend(entries);
        self.cell_of.clear();
        for e in &all {
            let c = self.cell(e.x, e.z);
            self.cell_of.push(c as u32);
            self.start[c + 1] += 1;
        }
        for i in 0..cells {
            self.start[i + 1] += self.start[i];
        }
        self.next.clear();
        self.next.extend_from_slice(&self.start[..cells]);
        let filler = all.first().copied().unwrap_or(Entry {
            entity: Entity::PLACEHOLDER,
            x: 0.0,
            y: 0.0,
            z: 0.0,
            half: 0.0,
            height: 0.0,
            asleep: false,
            until: 0,
        });
        self.items.clear();
        self.items.resize(all.len(), filler);
        for (e, &c) in all.iter().zip(&self.cell_of) {
            let slot = self.next[c as usize];
            self.next[c as usize] += 1;
            self.items[slot as usize] = *e;
        }
        self.scratch = all;
    }
    /// Calls `f` for every entry in the columns that meet the square.
    pub fn for_region(&self, x0: f64, z0: f64, x1: f64, z1: f64, mut f: impl FnMut(&Entry)) {
        let w = self.width as i64;
        for zz in self.column(z0).max(0)..=self.column(z1).min(w - 1) {
            for xx in self.column(x0).max(0)..=self.column(x1).min(w - 1) {
                let c = (zz * w + xx) as usize;
                for e in &self.items[self.start[c] as usize..self.start[c + 1] as usize] {
                    f(e);
                }
            }
        }
    }
    /// Calls `f` for the entries in the columns around `(x, z)` (3 x 3, a box is at most 2 blocks wide).
    pub fn for_near(&self, x: f64, z: f64, mut f: impl FnMut(&Entry)) {
        let (cx, cz) = (self.column(x), self.column(z));
        let w = self.width as i64;
        for zz in (cz - 1).max(0)..=(cz + 1).min(w - 1) {
            for xx in (cx - 1).max(0)..=(cx + 1).min(w - 1) {
                let c = (zz * w + xx) as usize;
                for e in &self.items[self.start[c] as usize..self.start[c + 1] as usize] {
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
}
#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
pub struct CowTick;

/// A walled stone slab of `half` blocks each way from the origin, on the prepared vanilla data.
pub fn build_level(half: i32) -> (Level, Species, Arc<horizon_client_lib::mobs::MobType>) {
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
                blocks.set(x, 0, z, stone).unwrap();
                if x.abs() == half || z.abs() == half {
                    blocks.set(x, 1, z, stone).unwrap();
                    blocks.set(x, 2, z, stone).unwrap();
                }
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
    };
    (Level { mw: Mw(mw), half }, species, kind)
}

pub fn cow_bundle(x: f64, z: f64, seed: u64) -> impl Bundle {
    let mut rng = Rng(seed | 1);
    let wait = idle_wait(&mut rng, 60);
    (
        Pos(Vec3::new(x, 1.0, z)),
        Vel(Vec3::ZERO),
        Yaw((rng.next_u32() % 360) as f32),
        Body { width: 0.9, height: 1.4, step_height: 0.6, gravity: 0.08, speed: 0.2 },
        Contact { on_ground: true, horizontal: false, fall_distance: 0.0 },
        Drive::default(),
        Sizes { adult: (0.9, 1.4), baby: (0.45, 0.7) },
        MoveTo::default(),
        Nav::default(),
        ActiveUntil(100),
        Stroll { wait, walking: false, pending: false, tx: 0.0, tz: 0.0, horizontal_range: 10, vertical_range: 7, interval: 60 },
        rng,
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
    s.add_systems((due, index, player_near, stroll, pathfind, follow, move_control, travel, push, resume, sleep).chain());
    s
}

// ---- systems ---------------------------------------------------------------------------------------------
/// `RandomStrollGoal.canUse` and `LandRandomPos`: after the wait, 10 tries of a spot within 10 x 7 (the first valid
/// one wins as all the ground weighs the same); the path to it is made by `pathfind`.
fn stroll_one(level: &Level, now: u64, active: &ActiveUntil, pos: &Pos, s: &mut Stroll, r: &mut Rng) {
    if s.walking || now >= active.0 {
        return;
    }
    if s.wait > 0 {
        s.wait -= 1;
        return;
    }
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
            s.pending = true;
            s.tx = f64::from(x) + 0.5;
            s.tz = f64::from(z) + 0.5;
            return;
        }
    }
    s.wait = idle_wait(r, s.interval);
}
fn stroll(
    cfg: Res<Config>,
    level: Res<Level>,
    clock: Res<Clock>,
    mut q: Query<(&Pos, &ActiveUntil, &mut Stroll, &mut Rng), Without<Asleep>>,
) {
    let now = clock.0;
    if cfg.parallel {
        q.par_iter_mut().for_each(|(p, a, mut s, mut r)| stroll_one(&level, now, a, p, &mut s, &mut r));
    } else {
        for (p, a, mut s, mut r) in &mut q {
            stroll_one(&level, now, a, p, &mut s, &mut r);
        }
    }
}

/// `Mob.checkDespawn`'s reset of `noActionTime`, by event: nothing is read while the players stay where they are.
/// When one moves (`Players.dirty`), the cows of the cells its old and new circles cover are tested against the
/// players of the 3 x 3 bins of 32 blocks around them, and only the cows that enter or leave the 32 blocks are
/// touched: an entering cow is active without end (`ActiveUntil(u64::MAX)`) and an idle sleeper wakes; a leaving
/// cow stays active for 100 ticks more (the `noActionTime` it has to count up to).
fn player_near(
    clock: Res<Clock>,
    mut players: ResMut<Players>,
    mut grid: ResMut<Grid>,
    mut active: Query<(&mut ActiveUntil, Option<&Asleep>)>,
    mut commands: Commands,
) {
    if !players.dirty {
        return;
    }
    players.dirty = false;
    const BIN: f64 = 32.0;
    let origin = grid.origin;
    let bins = ((grid.width as f64 * CELL / BIN).ceil() as i64).max(1) + 2;
    let bin = |v: f64| (((v - origin) / BIN).floor() as i64 + 1).clamp(0, bins - 1);
    let fill = |list: &[(f64, f64)]| {
        let mut by_bin: Vec<Vec<(f64, f64)>> = vec![Vec::new(); (bins * bins) as usize];
        for &(px, pz) in list {
            by_bin[(bin(pz) * bins + bin(px)) as usize].push((px, pz));
        }
        by_bin
    };
    let new = players.list.clone();
    let old = std::mem::replace(&mut players.prev, new.clone());
    let within = |by_bin: &Vec<Vec<(f64, f64)>>, x: f64, z: f64| {
        let (bx, bz) = (bin(x), bin(z));
        (bz - 1..=bz + 1).any(|zz| {
            (bx - 1..=bx + 1).any(|xx| {
                (0..bins).contains(&xx)
                    && (0..bins).contains(&zz)
                    && by_bin[(zz * bins + xx) as usize].iter().any(|&(px, pz)| (x - px).powi(2) + (z - pz).powi(2) < 32.0 * 32.0)
            })
        })
    };
    let (new_bins, _old_bins) = (fill(&new), ());
    // The cells either circle covers, each once.
    let tick = clock.0 as u32 + 1;
    let cells = grid.width * grid.width;
    let grid = &mut *grid;
    grid.stamp.resize(cells, 0);
    let mut covered: Vec<usize> = Vec::new();
    for &(px, pz) in new.iter().chain(old.iter()) {
        let (x0, x1) = (grid.column(px - 32.0).max(0), grid.column(px + 32.0).min(grid.width as i64 - 1));
        let (z0, z1) = (grid.column(pz - 32.0).max(0), grid.column(pz + 32.0).min(grid.width as i64 - 1));
        for zz in z0..=z1 {
            for xx in x0..=x1 {
                let c = zz as usize * grid.width + xx as usize;
                if grid.stamp[c] != tick {
                    grid.stamp[c] = tick;
                    covered.push(c);
                }
            }
        }
    }
    for c in covered {
        for e in &grid.items[grid.start[c] as usize..grid.start[c + 1] as usize] {
            let now_in = within(&new_bins, e.x, e.z);
            let Ok((mut a, asleep)) = active.get_mut(e.entity) else { continue };
            if now_in && a.0 != u64::MAX {
                a.0 = u64::MAX;
                if let Some(asleep) = asleep
                    && asleep.until == u64::MAX
                {
                    commands.entity(e.entity).remove::<Asleep>().insert(Resume(1));
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
/// `PathNavigation.createPath` + `moveTo` for the spot a stroll picked: Horizon's real path finder.
fn pathfind_one(now: u64, level: &Level, sp: &Species, pos: &Pos, body: &Body, c: &Contact, s: &mut Stroll, nav: &mut Nav, r: &mut Rng) {
    s.pending = false;
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
    match path {
        Some(p) if !p.nodes.is_empty() => {
            nav.path = Some(p);
            nav.last_check = now;
            nav.last_pos = pos.0;
        }
        _ => {
            nav.path = None;
            s.walking = false;
            s.wait = idle_wait(r, s.interval);
        }
    }
}
fn pathfind(
    cfg: Res<Config>,
    clock: Res<Clock>,
    level: Res<Level>,
    sp: Res<Species>,
    mut q: Query<(&Pos, &Body, &Contact, &mut Stroll, &mut Nav, &mut Rng), Without<Asleep>>,
) {
    if cfg.parallel {
        q.par_iter_mut().for_each(|(p, b, c, mut s, mut n, mut r)| {
            if s.pending {
                pathfind_one(clock.0, &level, &sp, p, b, c, &mut s, &mut n, &mut r);
            }
        });
    } else {
        for (p, b, c, mut s, mut n, mut r) in &mut q {
            if s.pending {
                pathfind_one(clock.0, &level, &sp, p, b, c, &mut s, &mut n, &mut r);
            }
        }
    }
}

/// `PathNavigation.tick` -> `followThePath` (ground): reach the waypoint, or cut the corner, then steer the move
/// control at the next node. (The stuck detection and the recompute on a block change are not here.)
fn follow_one(now: u64, level: &Level, pos: &Pos, body: &Body, drive: &Drive, s: &mut Stroll, nav: &mut Nav, m: &mut MoveTo, r: &mut Rng) {
    let mw = level.mw.0;
    let Some(path) = nav.path.as_mut() else {
        s.walking = false;
        s.wait = idle_wait(r, s.interval);
        return;
    };
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
    // `doStuckDetection`: every 100 ticks, a cow that has moved less than a quarter of what its speed would carry it
    // gives its path up.
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
    if stuck || path.is_done() {
        nav.path = None;
        s.walking = false;
        s.wait = idle_wait(r, s.interval);
        return;
    }
    let target = path.entity_pos_at(path.next, width);
    let (bx, by, bz) = (target.x.floor() as i32, target.y.floor() as i32, target.z.floor() as i32);
    let y = if mw.facts.is_air(mw.world.block(bx, by.saturating_sub(1), bz)) {
        target.y
    } else {
        mw.floor_level(bx, by, bz)
    };
    *m = MoveTo { active: true, x: target.x, y, z: target.z, speed_modifier: 1.0 };
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
fn follow(
    cfg: Res<Config>,
    clock: Res<Clock>,
    level: Res<Level>,
    mut q: Query<(&Pos, &Body, &Drive, &mut Stroll, &mut Nav, &mut MoveTo, &mut Rng), Without<Asleep>>,
) {
    let now = clock.0;
    if cfg.parallel {
        q.par_iter_mut().for_each(|(p, b, d, mut s, mut n, mut m, mut r)| {
            if s.walking && !s.pending {
                follow_one(now, &level, p, b, d, &mut s, &mut n, &mut m, &mut r);
            }
        });
    } else {
        for (p, b, d, mut s, mut n, mut m, mut r) in &mut q {
            if s.walking && !s.pending {
                follow_one(now, &level, p, b, d, &mut s, &mut n, &mut m, &mut r);
            }
        }
    }
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
fn move_control(cfg: Res<Config>, mut q: Query<(&Pos, &mut Yaw, &mut MoveTo, &mut Drive, &Body), Without<Asleep>>) {
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
fn travel_one(
    level: &Level,
    pos: &mut Pos,
    vel: &mut Vel,
    yaw: &Yaw,
    c: &mut Contact,
    d: &Drive,
    body: &Body,
) {
    // A cow at its resting state stays as it is: no sweep is made for it.
    if c.on_ground && d.zza == 0.0 && vel.0.x == 0.0 && vel.0.z == 0.0 && vel.0.y == (0.0 - body.gravity) * f64::from(0.98_f32) {
        return;
    }
    // `LivingEntity.aiStep`: the tiny motions are dropped.
    let v = vel.0;
    vel.0 = Vec3::new(
        if v.x.abs() < 0.003 { 0.0 } else { v.x },
        if v.y.abs() < 0.003 { 0.0 } else { v.y },
        if v.z.abs() < 0.003 { 0.0 } else { v.z },
    );
    let friction: f32 = if c.on_ground { 0.6 } else { 1.0 };
    let speed = if c.on_ground {
        if friction > 0.6 { d.speed * (0.216_000_02 / (friction * friction * friction)) } else { d.speed }
    } else {
        0.02
    };
    // `Entity.moveRelative` with the input (0, 0, zza).
    let input = Vec3::new(0.0, 0.0, f64::from(d.zza));
    let len_sq = input.x * input.x + input.y * input.y + input.z * input.z;
    if len_sq >= 1.0e-7 {
        let l = len_sq.sqrt();
        let i = if len_sq > 1.0 { Vec3::new(input.x / l, input.y / l, input.z / l) } else { input };
        let i = Vec3::new(i.x * f64::from(speed), i.y * f64::from(speed), i.z * f64::from(speed));
        let angle = f64::from(yaw.0 * trig::DEG_TO_RAD);
        let (s, co) = (f64::from(trig::sin(angle)), f64::from(trig::cos(angle)));
        let u = vel.0;
        vel.0 = Vec3::new(u.x + (i.x * co - i.z * s), u.y + i.y, u.z + (i.z * co + i.x * s));
    }
    // `Entity.move`: the sweep over the blocks, then the collision flags and the landing.
    let wanted = vel.0;
    let p = pos.0;
    let (hw, h) = (f64::from(body.width) / 2.0, f64::from(body.height));
    let bounds = Aabb::new(Vec3::new(p.x - hw, p.y, p.z - hw), Vec3::new(p.x + hw, p.y + h, p.z + hw));
    let scene = SceneCollisions { world: level.mw.0.world, binding: level.mw.0.collision, subject: None };
    let moved = scene
        .resolve_in_border_with(bounds, wanted, body.step_height, c.on_ground, None, &[])
        .unwrap_or(Vec3::ZERO);
    pos.0 = Vec3::new(p.x + moved.x, p.y + moved.y, p.z + moved.z);
    let differs = |a: f64, b: f64| (b - a).abs() >= f64::from(1.0e-5_f32);
    c.horizontal = differs(wanted.x, moved.x) || differs(wanted.z, moved.z);
    let vertical = wanted.y != moved.y;
    let below = vertical && wanted.y < 0.0;
    c.on_ground = below;
    if !c.on_ground && moved.y < 0.0 {
        c.fall_distance -= moved.y;
    }
    if c.on_ground {
        c.fall_distance = 0.0;
    }
    // `Entity.move` ends with the restitution after a landing: the vertical velocity is lost.
    let mut movement = vel.0;
    if vertical && below {
        movement.y = 0.0;
    }
    let drag = 0.91_f32;
    let f = friction * drag;
    let y = movement.y - body.gravity;
    vel.0 = Vec3::new(movement.x * f64::from(f), y * f64::from(0.98_f32), movement.z * f64::from(f));
}
fn travel(
    cfg: Res<Config>,
    level: Res<Level>,
    mut q: Query<(&mut Pos, &mut Vel, &Yaw, &mut Contact, &Drive, &Body), Without<Asleep>>,
) {
    if cfg.parallel {
        q.par_iter_mut().for_each(|(mut p, mut v, y, mut c, d, b)| travel_one(&level, &mut p, &mut v, y, &mut c, d, b));
    } else {
        for (mut p, mut v, y, mut c, d, b) in &mut q {
            travel_one(&level, &mut p, &mut v, y, &mut c, d, b);
        }
    }
}

/// The spatial index of the start of the step (a resource, rebuilt by a system).
fn index(mut grid: ResMut<Grid>, q: Query<(Entity, &Pos, &Body, Option<&Asleep>)>) {
    grid.rebuild(q.iter().map(|(entity, p, b, asleep)| Entry {
        entity,
        x: p.0.x,
        y: p.0.y,
        z: p.0.z,
        half: f64::from(b.width) / 2.0,
        height: f64::from(b.height),
        asleep: asleep.is_some(),
        until: asleep.map_or(0, |a| a.until),
    }));
}

/// `LivingEntity.pushEntities` / `Entity.push`, own half only: each cow reads the neighbours it overlaps in
/// the index (their positions at the start of the step) and moves its own velocity away from each; a sleeping
/// neighbour it overlaps is woken (it pushes itself the next step: a one-step delay).
fn push_one(
    grid: &Grid,
    now: u64,
    e: Entity,
    pos: &Pos,
    vel: &mut Vel,
    body: &Body,
    wake: &mut impl FnMut(Entity, i32),
) {
    let half = f64::from(body.width) / 2.0;
    let height = f64::from(body.height);
    let mut dv = (0.0_f64, 0.0_f64);
    grid.for_near(pos.0.x, pos.0.z, |o| {
        if o.entity == e {
            return;
        }
        let overlap = (pos.0.x - o.x).abs() < half + o.half
            && (pos.0.z - o.z).abs() < half + o.half
            && pos.0.y < o.y + o.height
            && o.y < pos.0.y + height;
        if !overlap {
            return;
        }
        if o.asleep {
            wake(o.entity, if o.until == u64::MAX { 1 } else { o.until.saturating_sub(now) as i32 });
        }
        let (mut xa, mut za) = (o.x - pos.0.x, o.z - pos.0.z);
        let mut dd = xa.abs().max(za.abs());
        if dd >= f64::from(0.01_f32) {
            dd = dd.sqrt();
            xa /= dd;
            za /= dd;
            let pow = (1.0_f64 / dd).min(1.0);
            xa *= pow * f64::from(0.05_f32);
            za *= pow * f64::from(0.05_f32);
            dv.0 -= xa;
            dv.1 -= za;
        }
    });
    if dv != (0.0, 0.0) {
        vel.0 = Vec3::new(vel.0.x + dv.0, vel.0.y, vel.0.z + dv.1);
    }
}
fn push(
    cfg: Res<Config>,
    grid: Res<Grid>,
    clock: Res<Clock>,
    par: ParallelCommands,
    mut q: Query<(Entity, &Pos, &mut Vel, &Body), Without<Asleep>>,
) {
    let now = clock.0;
    if cfg.parallel {
        q.par_iter_mut().for_each(|(e, p, mut v, b)| {
            par.command_scope(|mut c| {
                push_one(&grid, now, e, p, &mut v, b, &mut |o, remaining| {
                    c.entity(o).remove::<Asleep>().insert(Resume(remaining));
                })
            });
        });
    } else {
        par.command_scope(|mut c| {
            for (e, p, mut v, b) in &mut q {
                push_one(&grid, now, e, p, &mut v, b, &mut |o, remaining| {
                    c.entity(o).remove::<Asleep>().insert(Resume(remaining));
                });
            }
        });
    }
}
/// A sleeper woken by a push takes up its stroll wait where it was.
fn resume(mut q: Query<(Entity, &Resume, &mut Stroll)>, mut commands: Commands) {
    for (e, r, mut s) in &mut q {
        s.wait = r.0.max(1);
        commands.entity(e).remove::<Resume>();
    }
}

/// A cow at its resting state (on the ground, no horizontal motion, the vertical rest velocity, no walk, no
/// input, a stroll wait still to run) goes to sleep until that wait ends: the tick would leave it as it is.
fn sleep(
    cfg: Res<Config>,
    clock: Res<Clock>,
    mut wheel: ResMut<Wheel>,
    q: Query<(Entity, &Vel, &Contact, &Stroll, &Drive, &Body, &ActiveUntil), Without<Asleep>>,
    mut commands: Commands,
) {
    if !cfg.sleep {
        return;
    }
    for (e, v, c, s, d, b, a) in &q {
        let rest_vy = (0.0 - b.gravity) * f64::from(0.98_f32);
        if !(c.on_ground && !s.walking && d.zza == 0.0 && v.0.x == 0.0 && v.0.z == 0.0 && v.0.y == rest_vy) {
            continue;
        }
        if clock.0 >= a.0 {
            // The stroll goal is refused until a player comes near: no wake time, only the player wakes it.
            commands.entity(e).insert(Asleep { until: u64::MAX });
        } else if s.wait > 1 {
            let until = clock.0 + (s.wait as u64 - 1).min(WHEEL as u64 - 1);
            wheel.push(until, e, Due::Wake { until });
            commands.entity(e).insert(Asleep { until });
        }
    }
}
/// What is due at this tick in the timer wheel: a sleeper's stroll wait ends (it wakes owing 1), a baby grows up.
/// A sleeper that was woken earlier (a push) has a stale entry, ignored.
fn due(
    mut clock: ResMut<Clock>,
    mut wheel: ResMut<Wheel>,
    mut sleepers: Query<(&Asleep, &mut Stroll)>,
    mut babies: Query<(&mut Body, &Sizes, &GrowAt)>,
    mut commands: Commands,
) {
    clock.0 += 1;
    let slot = (clock.0 % WHEEL as u64) as usize;
    let list = std::mem::take(&mut wheel.slots[slot]);
    for (e, d) in list {
        match d {
            Due::Wake { until } => {
                if until == clock.0
                    && let Ok((a, mut s)) = sleepers.get_mut(e)
                    && a.until == until
                {
                    s.wait = 1;
                    commands.entity(e).remove::<Asleep>();
                }
            }
            Due::Grow => {
                if let Ok((mut b, sizes, g)) = babies.get_mut(e)
                    && g.0 == clock.0
                {
                    b.width = sizes.adult.0;
                    b.height = sizes.adult.1;
                    commands.entity(e).remove::<GrowAt>();
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
        let side = (n as f64).sqrt().ceil() as i32;
        let half = ((f64::from(side) * spacing / 2.0) as i32 + 6).max(20);
        let mut world = World::new();
        let (level, species, _kind) = build_level(half);
        world.insert_resource(level);
        world.insert_resource(species);
        world.insert_resource(Config { sleep, parallel });
        world.insert_resource(Clock::default());
        world.insert_resource(Players::default());
        world.insert_resource(Wheel::default());
        world.insert_resource(Grid { origin: -f64::from(half), width: (2 * half / 2) as usize + 2, ..Default::default() });
        let mut rng = Rng(seed | 1);
        for i in 0..n {
            let (cx, cz) = ((i as i32 % side) as f64, (i as i32 / side) as f64);
            let o = -f64::from(side) * spacing / 2.0;
            let s = u64::from(rng.next_u32()) << 32 | u64::from(rng.next_u32());
            world.spawn(cow_bundle(o + cx * spacing + 0.5, o + cz * spacing + 0.5, s));
        }
        Self { world, schedule: schedule() }
    }
    /// The players of the herd's world (their positions; cows within 32 blocks stay active).
    pub fn set_players(&mut self, players: Vec<(f64, f64)>) {
        let mut p = self.world.resource_mut::<Players>();
        p.list = players;
        p.dirty = true;
    }
    pub fn tick(&mut self) {
        self.schedule.run(&mut self.world);
    }
    /// The cow grows up in `ticks` ticks (it is a baby until then: the baby's size, one wheel entry).
    pub fn make_baby(&mut self, e: Entity, ticks: u64) {
        let now = self.world.resource::<Clock>().0;
        let sizes = *self.world.get::<Sizes>(e).unwrap();
        let mut b = self.world.get_mut::<Body>(e).unwrap();
        b.width = sizes.baby.0;
        b.height = sizes.baby.1;
        self.world.entity_mut(e).insert(GrowAt(now + ticks));
        self.world.resource_mut::<Wheel>().push(now + ticks, e, Due::Grow);
    }
    /// One tick with the wall time of each system added to `acc` (seconds), in the schedule's order.
    pub fn tick_timed(&mut self, acc: &mut [f64; 11]) {
        macro_rules! t {
            ($i:expr, $s:expr) => {
                let t0 = std::time::Instant::now();
                self.world.run_system_cached($s).unwrap();
                acc[$i] += t0.elapsed().as_secs_f64();
            };
        }
        t!(0, due);
        t!(1, index);
        t!(2, player_near);
        t!(3, stroll);
        t!(4, pathfind);
        t!(5, follow);
        t!(6, move_control);
        t!(7, travel);
        t!(8, push);
        t!(9, resume);
        t!(10, sleep);
    }
    pub const SYSTEMS: [&'static str; 11] =
        ["due", "index", "player_near", "stroll", "pathfind", "follow", "move_control", "travel", "push", "resume", "sleep"];
    /// (asleep, walking, total)
    pub fn counts(&mut self) -> (usize, usize, usize) {
        let asleep = self.world.query_filtered::<(), With<Asleep>>().iter(&self.world).count();
        let walking = self.world.query::<&Stroll>().iter(&self.world).filter(|s| s.walking).count();
        let total = self.world.query::<&Pos>().iter(&self.world).count();
        (asleep, walking, total)
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
                let (asleep, _, _) = herd.counts();
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
        {
            let mut q = herd.world.query_filtered::<(&Stroll, &Vel, &Contact, &Pos, &ActiveUntil), Without<Asleep>>();
            for (s, v, c, p, a) in q.iter(&herd.world).take(6) {
                eprintln!("awake: walking={} pending={} wait={} vel=({:.4},{:.4},{:.4}) ground={} pos=({:.1},{:.1}) active_until={}", s.walking, s.pending, s.wait, v.0.x, v.0.y, v.0.z, c.on_ground, p.0.x, p.0.z, a.0);
            }
        }
        assert!(herd.counts().0 >= 295, "{} asleep after the player left", herd.counts().0);
        // And the player comes back: the cows around it wake.
        herd.set_players(vec![(0.0, 0.0)]);
        for _ in 0..5 {
            herd.tick();
        }
        assert!(herd.counts().0 < 150, "{} still asleep with a player there", herd.counts().0);
        eprintln!("travelled {travelled}, asleep between {min_asleep} and {max_asleep} of 300");
    }
}
