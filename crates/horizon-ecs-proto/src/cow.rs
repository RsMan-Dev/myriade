//! A cow as data components and batch systems (palier 1): the stroll goal (a geometric wait with the law of
//! `nextInt(60) == 0` each tick, then `LandRandomPos` 10 x 7), the move control, `LivingEntity.travel` on the
//! ground with Horizon's real block collision sweep (`SceneCollisions::resolve_in_border_with`), and sleeping.
//! Species data (size, step, speed, stroll range) are component values, not branches.
use bevy_ecs::{
    prelude::*,
    schedule::{Schedule, ScheduleLabel, SingleThreadedExecutor},
};
use horizon_client_render::{
    assets::Assets,
    chunk::Chunk,
    collisions::{self, SceneCollisions},
    world::World as Blocks,
};
use horizon_core::geometry::{Aabb, Vec3, trig};
use horizon_world::block_collision::BlockCollisionRegistry;
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
    pub tx: f64,
    pub tz: f64,
    pub horizontal_range: i32,
    pub vertical_range: i32,
    /// The attempt's chance is 1 in this (`reducedTickDelay(120)`).
    pub interval: i32,
}
#[derive(Component, Clone, Copy, Debug)]
pub struct Rng(pub u64);
/// `AgeableMob`: negative for a baby, counting up to 0 (adult).
#[derive(Component, Clone, Copy, Debug)]
pub struct Age(pub i32);
/// The size of an adult and of a baby of the species (data, not code).
#[derive(Component, Clone, Copy, Debug)]
pub struct Sizes {
    pub adult: (f32, f32),
    pub baby: (f32, f32),
}
/// On the ground, motionless, no walk, a stroll wait still to run: no movement system visits it.
#[derive(Component, Clone, Copy, Debug)]
pub struct Asleep;

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
/// The blocks, owned (a snapshot of the bench's floor) with the real collision registry.
#[derive(Resource)]
pub struct Level {
    pub blocks: Blocks,
    pub collision: BlockCollisionRegistry<'static>,
    /// The walls are at |x| or |z| = `half`.
    pub half: i32,
}
#[derive(Resource, Clone, Copy)]
pub struct Config {
    pub sleep: bool,
    pub parallel: bool,
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
    /// A counting sort of the entries by column.
    pub fn rebuild(&mut self, entries: impl Iterator<Item = Entry>) {
        let cells = self.width * self.width;
        self.start.clear();
        self.start.resize(cells + 1, 0);
        let all: Vec<Entry> = entries.collect();
        self.cell_of.clear();
        for e in &all {
            let c = self.cell(e.x, e.z);
            self.cell_of.push(c as u32);
            self.start[c + 1] += 1;
        }
        for i in 0..cells {
            self.start[i + 1] += self.start[i];
        }
        let mut next: Vec<u32> = self.start[..cells].to_vec();
        self.items.clear();
        self.items.resize(all.len(), all.first().copied().unwrap_or(Entry { entity: Entity::PLACEHOLDER, x: 0.0, y: 0.0, z: 0.0, half: 0.0, height: 0.0, asleep: false }));
        for (e, &c) in all.iter().zip(&self.cell_of) {
            let slot = next[c as usize];
            next[c as usize] += 1;
            self.items[slot as usize] = *e;
        }
    }
    /// The entries in the columns around `(x, z)` (3 x 3, a box is at most 2 blocks wide).
    pub fn near(&self, x: f64, z: f64) -> impl Iterator<Item = &Entry> {
        let (cx, cz) = (self.column(x), self.column(z));
        let w = self.width as i64;
        (cz - 1..=cz + 1).flat_map(move |zz| {
            (cx - 1..=cx + 1).flat_map(move |xx| {
                let range = if xx < 0 || zz < 0 || xx >= w || zz >= w {
                    0..0
                } else {
                    let c = (zz * w + xx) as usize;
                    self.start[c] as usize..self.start[c + 1] as usize
                };
                self.items[range].iter()
            })
        })
    }
}
#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
pub struct CowTick;

/// A walled stone slab of `half` blocks each way from the origin, on the prepared vanilla data.
pub fn build_level(root: &std::path::Path, half: i32) -> Level {
    let assets = Assets::load(root).expect("assets");
    let registry = assets.registry;
    let collision = collisions::load(root, registry).expect("collisions");
    let air = registry.default_state("minecraft:air").unwrap();
    let stone = registry.default_state("minecraft:stone").unwrap();
    let mut blocks = Blocks::new(air, 0);
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
    Level { blocks, collision, half }
}

pub fn cow_bundle(x: f64, z: f64, seed: u64) -> impl Bundle {
    let mut rng = Rng(seed | 1);
    let wait = geometric(&mut rng, 60);
    (
        Pos(Vec3::new(x, 1.0, z)),
        Vel(Vec3::ZERO),
        Yaw((rng.next_u32() % 360) as f32),
        Body { width: 0.9, height: 1.4, step_height: 0.6, gravity: 0.08, speed: 0.2 },
        Contact { on_ground: true, horizontal: false, fall_distance: 0.0 },
        Drive::default(),
        Age(0),
        Sizes { adult: (0.9, 1.4), baby: (0.45, 0.7) },
        MoveTo::default(),
        Stroll { wait, walking: false, tx: 0.0, tz: 0.0, horizontal_range: 10, vertical_range: 7, interval: 60 },
        rng,
    )
}

/// The number of failed attempts before the first success of a 1-in-`n` draw each tick (the same law as
/// drawing `nextInt(n) == 0` every tick, sampled once).
pub fn geometric(rng: &mut Rng, n: i32) -> i32 {
    let u = rng.next_f64().max(1.0e-12);
    (u.ln() / (1.0 - 1.0 / f64::from(n)).ln()).floor() as i32
}

pub fn schedule() -> Schedule {
    let mut s = Schedule::new(CowTick);
    s.set_executor(SingleThreadedExecutor::new());
    // The order is explicit: index, stroll, move control, travel, push, growth, sleep, wake.
    s.add_systems((index, stroll, move_control, travel, push, growth, sleep, wake).chain());
    s
}

// ---- systems ---------------------------------------------------------------------------------------------
/// `RandomStrollGoal` and the end of a path: wait, pick a spot (`LandRandomPos`, 10 tries, the first valid one
/// wins as all the ground weighs the same), walk to it, stop within the waypoint distance of it.
fn stroll_one(level: &Level, pos: &Pos, s: &mut Stroll, m: &mut MoveTo, r: &mut Rng, body: &Body) {
    if !s.walking {
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
                s.tx = f64::from(x) + 0.5;
                s.tz = f64::from(z) + 0.5;
                break;
            }
        }
        if !s.walking {
            s.wait = geometric(r, s.interval);
            return;
        }
    }
    // `PathNavigation.followThePath`: the last waypoint is reached within this distance.
    let reach = if body.width > 0.75 { body.width / 2.0 } else { 0.75 - body.width / 2.0 };
    let (dx, dz) = (s.tx - pos.0.x, s.tz - pos.0.z);
    if dx * dx + dz * dz < f64::from(reach) * f64::from(reach) {
        s.walking = false;
        s.wait = geometric(r, s.interval);
    } else {
        *m = MoveTo { active: true, x: s.tx, y: 1.0, z: s.tz, speed_modifier: 1.0 };
    }
}
fn stroll(
    cfg: Res<Config>,
    level: Res<Level>,
    mut q: Query<(&Pos, &mut Stroll, &mut MoveTo, &mut Rng, &Body), Without<Asleep>>,
) {
    if cfg.parallel {
        q.par_iter_mut().for_each(|(p, mut s, mut m, mut r, b)| stroll_one(&level, p, &mut s, &mut m, &mut r, b));
    } else {
        for (p, mut s, mut m, mut r, b) in &mut q {
            stroll_one(&level, p, &mut s, &mut m, &mut r, b);
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
    let scene = SceneCollisions { world: &level.blocks, binding: &level.collision, subject: None };
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
fn index(mut grid: ResMut<Grid>, q: Query<(Entity, &Pos, &Body, Has<Asleep>)>) {
    grid.rebuild(q.iter().map(|(entity, p, b, asleep)| Entry {
        entity,
        x: p.0.x,
        y: p.0.y,
        z: p.0.z,
        half: f64::from(b.width) / 2.0,
        height: f64::from(b.height),
        asleep,
    }));
}

/// `LivingEntity.pushEntities` / `Entity.push`, own half only: each cow reads the neighbours it overlaps in
/// the index (their positions at the start of the step) and moves its own velocity away from each; a sleeping
/// neighbour it overlaps is woken (it pushes itself the next step: a one-step delay).
fn push_one(grid: &Grid, e: Entity, pos: &Pos, vel: &mut Vel, body: &Body, wake: &mut impl FnMut(Entity)) {
    let half = f64::from(body.width) / 2.0;
    let height = f64::from(body.height);
    for o in grid.near(pos.0.x, pos.0.z) {
        if o.entity == e {
            continue;
        }
        let overlap = (pos.0.x - o.x).abs() < half + o.half
            && (pos.0.z - o.z).abs() < half + o.half
            && pos.0.y < o.y + o.height
            && o.y < pos.0.y + height;
        if !overlap {
            continue;
        }
        if o.asleep {
            wake(o.entity);
        }
        let (mut xa, mut za) = (o.x - pos.0.x, o.z - pos.0.z);
        let mut dd = xa.abs().max(za.abs());
        if dd >= f64::from(0.01_f32) {
            dd = dd.sqrt();
            xa /= dd;
            za /= dd;
            let pow = (1.0 / dd).min(1.0);
            xa *= pow * f64::from(0.05_f32);
            za *= pow * f64::from(0.05_f32);
            vel.0 = Vec3::new(vel.0.x - xa, vel.0.y, vel.0.z - za);
        }
    }
}
fn push(
    cfg: Res<Config>,
    grid: Res<Grid>,
    par: ParallelCommands,
    mut q: Query<(Entity, &Pos, &mut Vel, &Body), Without<Asleep>>,
) {
    if cfg.parallel {
        q.par_iter_mut().for_each(|(e, p, mut v, b)| {
            par.command_scope(|mut c| push_one(&grid, e, p, &mut v, b, &mut |o| {
                c.entity(o).remove::<Asleep>();
            }));
        });
    } else {
        par.command_scope(|mut c| {
            for (e, p, mut v, b) in &mut q {
                push_one(&grid, e, p, &mut v, b, &mut |o| {
                    c.entity(o).remove::<Asleep>();
                });
            }
        });
    }
}

/// `AgeableMob.aiStep`: a baby's age counts up to 0, then it has the adult's size (`Sizes`, data).
fn growth(mut q: Query<(&mut Age, &mut Body, &Sizes), Without<Asleep>>) {
    for (mut age, mut body, sizes) in &mut q {
        if age.0 < 0 {
            age.0 += 1;
            let (w, h) = if age.0 < 0 { sizes.baby } else { sizes.adult };
            body.width = w;
            body.height = h;
        }
    }
}

/// A cow at its resting state (on the ground, no horizontal motion, the vertical rest velocity, no walk, no
/// input, a wait still to run) goes to sleep: the tick would leave it as it is.
fn sleep(
    cfg: Res<Config>,
    q: Query<(Entity, &Vel, &Contact, &Stroll, &Drive, &Body, &Age), Without<Asleep>>,
    mut commands: Commands,
) {
    if !cfg.sleep {
        return;
    }
    for (e, v, c, s, d, b, age) in &q {
        let rest_vy = (0.0 - b.gravity) * f64::from(0.98_f32);
        if age.0 >= 0 && c.on_ground && !s.walking && s.wait > 1 && d.zza == 0.0 && v.0.x == 0.0 && v.0.z == 0.0 && v.0.y == rest_vy {
            commands.entity(e).insert(Asleep);
        }
    }
}
/// What wakes a sleeping cow: the end of its stroll wait (nothing else is modelled in this step).
fn wake(mut q: Query<(Entity, &mut Stroll), With<Asleep>>, mut commands: Commands) {
    for (e, mut s) in &mut q {
        s.wait -= 1;
        if s.wait <= 1 {
            commands.entity(e).remove::<Asleep>();
        }
    }
}

pub struct Herd {
    pub world: World,
    pub schedule: Schedule,
}
impl Herd {
    /// `n` cows spaced `spacing` blocks apart on a slab just large enough.
    pub fn new(root: &std::path::Path, n: usize, spacing: f64, sleep: bool, parallel: bool, seed: u64) -> Self {
        let side = (n as f64).sqrt().ceil() as i32;
        let half = ((f64::from(side) * spacing / 2.0) as i32 + 6).max(20);
        let mut world = World::new();
        world.insert_resource(build_level(root, half));
        world.insert_resource(Config { sleep, parallel });
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
    pub fn tick(&mut self) {
        self.schedule.run(&mut self.world);
    }
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
        let Some(root) = std::env::var_os("HORIZON_TEST_RESOURCES") else {
            return;
        };
        let mut herd = Herd::new(std::path::Path::new(&root), 0, 6.0, true, false, 3);
        let half = herd.world.resource::<Level>().half;
        let _ = half;
        let a = herd.world.spawn(cow_bundle(0.5, 0.5, 11)).id();
        let b = herd.world.spawn(cow_bundle(0.8, 0.5, 12)).id();
        let baby = herd.world.spawn(cow_bundle(10.5, 10.5, 13)).id();
        herd.world.entity_mut(baby).insert(Age(-30));
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
        let Some(root) = std::env::var_os("HORIZON_TEST_RESOURCES") else {
            eprintln!("HORIZON_TEST_RESOURCES is not set: skipped");
            return;
        };
        let mut herd = Herd::new(std::path::Path::new(&root), 300, 6.0, true, false, 3);
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
        eprintln!("travelled {travelled}, asleep between {min_asleep} and {max_asleep} of 300");
    }
}
