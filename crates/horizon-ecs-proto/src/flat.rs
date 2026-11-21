//! Throwaway prototype (the design notes section 11): a herd of wandering cows as a `bevy_ecs` world.
//! Components are data only, the systems are batch functions chained in a `Schedule`, a cow at rest on the
//! ground gets the table component `Asleep` and leaves every movement query until its timer wakes it.
//! Not a cow: no pushes, no vanilla goals, no fluids, no blocks (the ground is the plane y = 0), no
//! health, no age, no collisions between cows. Nothing here is compared with `Mob` field by field.
use bevy_ecs::{
    prelude::*,
    schedule::{Schedule, ScheduleLabel, SingleThreadedExecutor},
};

#[derive(Component, Clone, Copy, Debug)]
pub struct Pos {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}
#[derive(Component, Clone, Copy, Debug)]
pub struct Vel {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}
#[derive(Component, Clone, Copy, Debug)]
pub struct Body {
    pub w: f32,
    pub h: f32,
    pub on_ground: bool,
}
#[derive(Component, Clone, Copy, Debug)]
pub struct Wander {
    pub timer: i32,
    pub has_target: bool,
    pub tx: f64,
    pub tz: f64,
}
#[derive(Component, Clone, Copy, Debug)]
pub struct Rng(pub u64);
/// A cow on the ground, motionless, whose timer has not run out: no movement system visits it.
#[derive(Component, Clone, Copy, Debug)]
pub struct Asleep;

impl Rng {
    pub fn next(&mut self) -> u32 {
        // xorshift64*
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        (self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 32) as u32
    }
}

#[derive(Resource, Clone, Copy)]
pub struct Config {
    /// Half the side of the slab, in blocks (the walls).
    pub half: f64,
    /// Whether a resting cow goes to sleep.
    pub sleep: bool,
    /// Whether the batch systems split their cows over the threads.
    pub parallel: bool,
}
#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Tick;

pub fn spawn_herd(world: &mut World, n: usize, half: f64, seed: u64) {
    let mut rng = Rng(seed | 1);
    for _ in 0..n {
        let x = (f64::from(rng.next()) / f64::from(u32::MAX) * 2.0 - 1.0) * (half - 1.0);
        let z = (f64::from(rng.next()) / f64::from(u32::MAX) * 2.0 - 1.0) * (half - 1.0);
        let timer = 1 + (rng.next() % 240) as i32;
        let r = Rng(u64::from(rng.next()) << 32 | u64::from(rng.next()) | 1);
        world.spawn((
            Pos { x, y: 0.0, z },
            Vel { x: 0.0, y: 0.0, z: 0.0 },
            Body { w: 0.9, h: 1.4, on_ground: true },
            Wander { timer, has_target: false, tx: 0.0, tz: 0.0 },
            r,
        ));
    }
}

pub fn schedule() -> Schedule {
    let mut s = Schedule::new(Tick);
    // The systems are chained, so the schedule runs them one after the other; the threads, when used, work
    // inside a system (`par_iter_mut`).
    s.set_executor(SingleThreadedExecutor::new());
    // The order is explicit: wander, gravity, move, friction, sleep, wake.
    s.add_systems((wander, gravity, movement, friction, sleep, wake).chain());
    s
}

/// A stroll, simplified: a timer; when it runs out the cow either stays idle for a while or picks a target
/// within 12 blocks and walks to it (speed 0.1 per tick at most), giving up when the timer runs out again.
fn wander_one(p: &Pos, v: &mut Vel, w: &mut Wander, r: &mut Rng, half: f64) {
    w.timer -= 1;
    if w.has_target {
        let (dx, dz) = (w.tx - p.x, w.tz - p.z);
        let d = (dx * dx + dz * dz).sqrt();
        if d < 0.6 || w.timer <= 0 {
            w.has_target = false;
            w.timer = 20 + (r.next() % 100) as i32;
        } else {
            v.x += dx / d * 0.03;
            v.z += dz / d * 0.03;
        }
    } else if w.timer <= 0 {
        if r.next() % 100 < 50 {
            let ax = (f64::from(r.next()) / f64::from(u32::MAX) * 2.0 - 1.0) * 12.0;
            let az = (f64::from(r.next()) / f64::from(u32::MAX) * 2.0 - 1.0) * 12.0;
            w.tx = (p.x + ax).clamp(-half + 1.0, half - 1.0);
            w.tz = (p.z + az).clamp(-half + 1.0, half - 1.0);
            w.has_target = true;
            w.timer = 200;
        } else {
            w.timer = 40 + (r.next() % 200) as i32;
        }
    }
}
fn wander(cfg: Res<Config>, mut q: Query<(&Pos, &mut Vel, &mut Wander, &mut Rng), Without<Asleep>>) {
    let half = cfg.half;
    if cfg.parallel {
        q.par_iter_mut().for_each(|(p, mut v, mut w, mut r)| wander_one(p, &mut v, &mut w, &mut r, half));
    } else {
        for (p, mut v, mut w, mut r) in &mut q {
            wander_one(p, &mut v, &mut w, &mut r, half);
        }
    }
}

fn gravity(cfg: Res<Config>, mut q: Query<&mut Vel, Without<Asleep>>) {
    if cfg.parallel {
        q.par_iter_mut().for_each(|mut v| v.y = (v.y - 0.08) * 0.98);
    } else {
        for mut v in &mut q {
            v.y = (v.y - 0.08) * 0.98;
        }
    }
}

fn move_one(p: &mut Pos, v: &mut Vel, b: &mut Body, half: f64) {
    p.x += v.x;
    p.y += v.y;
    p.z += v.z;
    // The ground is the plane y = 0 (the real block sweep is not used here).
    if p.y <= 0.0 {
        p.y = 0.0;
        if v.y < 0.0 {
            v.y = 0.0;
        }
        b.on_ground = true;
    } else {
        b.on_ground = false;
    }
    let lim = half - 0.5;
    if p.x.abs() > lim {
        p.x = p.x.clamp(-lim, lim);
        v.x = 0.0;
    }
    if p.z.abs() > lim {
        p.z = p.z.clamp(-lim, lim);
        v.z = 0.0;
    }
}
fn movement(cfg: Res<Config>, mut q: Query<(&mut Pos, &mut Vel, &mut Body), Without<Asleep>>) {
    let half = cfg.half;
    if cfg.parallel {
        q.par_iter_mut().for_each(|(mut p, mut v, mut b)| move_one(&mut p, &mut v, &mut b, half));
    } else {
        for (mut p, mut v, mut b) in &mut q {
            move_one(&mut p, &mut v, &mut b, half);
        }
    }
}

fn friction_one(v: &mut Vel, b: &Body) {
    let f = if b.on_ground { 0.91 * 0.6 } else { 0.91 };
    v.x *= f;
    v.z *= f;
    if v.x.abs() < 0.003 {
        v.x = 0.0;
    }
    if v.z.abs() < 0.003 {
        v.z = 0.0;
    }
}
fn friction(cfg: Res<Config>, mut q: Query<(&mut Vel, &Body), Without<Asleep>>) {
    if cfg.parallel {
        q.par_iter_mut().for_each(|(mut v, b)| friction_one(&mut v, &b));
    } else {
        for (mut v, b) in &mut q {
            friction_one(&mut v, &b);
        }
    }
}

/// A cow on the ground, motionless, with no target and a timer still to run goes to sleep.
fn sleep(
    cfg: Res<Config>,
    q: Query<(Entity, &Vel, &Body, &Wander), Without<Asleep>>,
    mut commands: Commands,
) {
    if !cfg.sleep {
        return;
    }
    for (e, v, b, w) in &q {
        if b.on_ground && !w.has_target && v.x == 0.0 && v.y == 0.0 && v.z == 0.0 && w.timer > 0 {
            commands.entity(e).insert(Asleep);
        }
    }
}
/// The only thing a sleeping cow does: count its timer down, and wake when it runs out.
fn wake(mut q: Query<(Entity, &mut Wander), With<Asleep>>, mut commands: Commands) {
    for (e, mut w) in &mut q {
        w.timer -= 1;
        if w.timer <= 0 {
            commands.entity(e).remove::<Asleep>();
        }
    }
}

pub struct Herd {
    pub world: World,
    pub schedule: Schedule,
}
impl Herd {
    pub fn new(n: usize, sleep: bool, parallel: bool, seed: u64) -> Self {
        let half = (32.0_f64).max((n as f64).sqrt() * 2.5);
        let mut world = World::new();
        world.insert_resource(Config { half, sleep, parallel });
        spawn_herd(&mut world, n, half, seed);
        Self { world, schedule: schedule() }
    }
    pub fn tick(&mut self) {
        self.schedule.run(&mut self.world);
    }
    /// (asleep, moving, total)
    pub fn counts(&mut self) -> (usize, usize, usize) {
        let asleep = self.world.query_filtered::<(), With<Asleep>>().iter(&self.world).count();
        let moving = self.world.query::<&Vel>().iter(&self.world).filter(|v| v.x != 0.0 || v.z != 0.0).count();
        let total = self.world.query::<&Pos>().iter(&self.world).count();
        (asleep, moving, total)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Common sense: the cows stay on the slab and on the ground, they move, they fall asleep, they wake.
    #[test]
    fn the_herd_moves_sleeps_and_wakes() {
        let mut herd = Herd::new(500, true, false, 7);
        let half = herd.world.resource::<Config>().half;
        let (mut max_asleep, mut min_asleep, mut moved) = (0, usize::MAX, false);
        let start: Vec<(f64, f64)> = herd.world.query::<&Pos>().iter(&herd.world).map(|p| (p.x, p.z)).collect();
        for t in 0..600 {
            herd.tick();
            if t >= 300 {
                let (asleep, _, _) = herd.counts();
                max_asleep = max_asleep.max(asleep);
                min_asleep = min_asleep.min(asleep);
            }
        }
        let now: Vec<(f64, f64)> = herd.world.query::<&Pos>().iter(&herd.world).map(|p| (p.x, p.z)).collect();
        for (a, b) in start.iter().zip(&now) {
            moved |= (a.0 - b.0).abs() > 0.5 || (a.1 - b.1).abs() > 0.5;
        }
        for (p, b) in herd.world.query::<(&Pos, &Body)>().iter(&herd.world) {
            assert!(p.x.abs() <= half && p.z.abs() <= half && p.y >= 0.0 && p.y < 1.0 && b.on_ground);
        }
        assert!(moved, "no cow moved");
        assert!(max_asleep > 100 && min_asleep < 500, "{min_asleep}..{max_asleep} asleep of 500");
        eprintln!("asleep between {min_asleep} and {max_asleep} of 500");
    }
}
