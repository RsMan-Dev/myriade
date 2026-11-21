//! `homing_projectile`: a projectile that steers on its target in axis-aligned steps (`selectNextMoveDirection`). Its steering is two systems
//! around the flight: `steer` before the move (the velocity turns toward the step's delta by `turn` a tick; the delta grows by `grow` a tick up
//! to `max_delta`), `follow` after it (the step count, the next direction when the step runs out or a block centre is reached with a stand spot).
//! With no target it falls by `gravity`. Its own xorshift stream (on `seed`) draws the directions. The configuration is the species' (`Config`);
//! the component keeps a reference to it. Not done here: the save of the projectile (`entity_nbt`, socle).
use super::*;
use crate::cow::*;
use crate::projectile::{Flight, STEPS, launch};
use crate::rules::homing_projectile::{grow_delta, turn_velocity, unit_delta, xorshift_draw};
use crate::targeting::{self, Targets, Who};
use horizon_core::geometry::Vec3;

/// The species' numbers for the projectile: its flight (damage, effect on hit) and the steering.
#[derive(Clone, Copy, Debug)]
pub struct Config {
    pub flight: Flight,
    pub gravity: f64,
    pub speed: f64,
    pub grow: f64,
    pub turn: f64,
    pub max_delta: f64,
    pub base_steps: i32,
    pub step_count: i32,
    pub step_len: i32,
}

#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet", on_add = crate::goals::homing_in, on_remove = crate::goals::homing_out)]
pub struct Homing {
    pub target: Who,
    pub dir: Option<u8>,
    pub steps: i32,
    pub delta: [f64; 3],
    pub seed: u64,
    pub cfg: &'static Config,
}

/// The homing step, called by the framework's tick only while a homing projectile lives (no registered system): `follow` finishes the
/// previous tick's flight (it ran after `fly`), then `steer` turns the velocity for this tick's move.
pub fn tick(w: &mut World) {
    crate::goals::tick_state(w, follow);
    crate::goals::tick_state(w, steer);
}

/// Launches a projectile from the middle of `owner`'s box, first leaving along an axis other than `axis` (0 x, 1 y, 2 z).
pub fn spawn(w: &mut World, owner: Entity, target: Who, axis: u8, seed: u64, cfg: &'static Config) {
    let (Some(p), Some(h)) = (w.get::<Pos>(owner).map(|p| p.0), w.get::<Body>(owner).map(|b| f64::from(b.height))) else {
        return;
    };
    let mut at = Vec3::new(p.x, p.y + h * 0.5, p.z);
    // `ShulkerBullet(level, owner, ..)` starts at the centre of the owner's box, which the open lid lengthens on its side.
    if let Some(a) = w.get::<crate::behaviours::attached_face::Attached>(owner) {
        let lift = f64::from(crate::behaviours::attached_face::physical_peek(a.eased)) * f64::from(crate::cow::model_scale(w, owner)) * 0.5;
        let s = STEPS[(a.face.min(5) ^ 1) as usize];
        at = Vec3::new(at.x + f64::from(s[0]) * lift, at.y + f64::from(s[1]) * lift, at.z + f64::from(s[2]) * lift);
    }
    let e = launch(w, cfg.flight, at, Vec3::ZERO, Some(Who::Entity(owner)));
    // Shot from outside the simulated chunks, a hurting projectile is already discarded.
    if w.get_entity(e).is_err() {
        return;
    }
    let born = w.resource::<crate::cow::Clock>().0;
    w.entity_mut(e).insert(crate::projectile::BornAt(born));
    w.entity_mut(e).insert(Homing { target, dir: Some(1), steps: 0, delta: [0.0; 3], seed: seed | 1, cfg });
    select_next(w, e, Some(axis));
}

fn draw(h: &mut Homing, bound: i32) -> i32 {
    let (seed, v) = xorshift_draw(h.seed, bound);
    h.seed = seed;
    v
}

fn cell(p: Vec3) -> [i32; 3] {
    [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32]
}

/// `selectNextMoveDirection`: toward the target's centre, the open directions along the axes still off; a random one when none is open.
fn select_next(w: &mut World, e: Entity, excluded: Option<u8>) {
    let Some(mut h) = w.get::<Homing>(e).copied() else { return };
    let box_ = targeting::info(w, h.target).map(|i| i.bounds);
    let (target, y_offset) = match box_ {
        Some(b) => {
            let (lo, hi) = (b.min_position(), b.max_position());
            let half = (hi.y - lo.y) * 0.5;
            let c = [(lo.x + hi.x) / 2.0, lo.y + half, (lo.z + hi.z) / 2.0];
            (c.map(|v| v.floor() as i32), half)
        }
        None => {
            let p = cell(w.get::<Pos>(e).map(|p| p.0).unwrap_or(Vec3::ZERO));
            ([p[0], p[1] - 1, p[2]], 0.5)
        }
    };
    let pos = w.get::<Pos>(e).map(|p| p.0).unwrap_or(Vec3::ZERO);
    let centre = [f64::from(target[0]) + 0.5, f64::from(target[1]) + 0.5, f64::from(target[2]) + 0.5];
    let d2 = (pos.x - centre[0]).powi(2) + (pos.y - centre[1]).powi(2) + (pos.z - centre[2]).powi(2);
    let mut goal = [f64::from(target[0]) + 0.5, f64::from(target[1]) + y_offset, f64::from(target[2]) + 0.5];
    let mut selection = None;
    if d2 >= 4.0 {
        let cur = cell(pos);
        let step = |d: u8| {
            let s = STEPS[d as usize];
            [cur[0] + s[0], cur[1] + s[1], cur[2] + s[2]]
        };
        let mut options = Vec::new();
        for (axis, (lo, hi)) in [(0u8, (4u8, 5u8)), (1, (0, 1)), (2, (2, 3))] {
            if excluded == Some(axis) {
                continue;
            }
            let i = axis as usize;
            if cur[i] < target[i] && block_empty(w, step(hi)) {
                options.push(hi);
            } else if cur[i] > target[i] && block_empty(w, step(lo)) {
                options.push(lo);
            }
        }
        let mut pick = draw(&mut h, 6) as u8;
        if options.is_empty() {
            let mut i = 5;
            while !block_empty(w, step(pick)) && i > 0 {
                pick = draw(&mut h, 6) as u8;
                i -= 1;
            }
        } else {
            pick = options[draw(&mut h, options.len() as i32) as usize];
        }
        let s = STEPS[pick as usize];
        goal = [pos.x + f64::from(s[0]), pos.y + f64::from(s[1]), pos.z + f64::from(s[2])];
        selection = Some(pick);
    }
    let d = [goal[0] - pos.x, goal[1] - pos.y, goal[2] - pos.z];
    h.delta = unit_delta(d, h.cfg.speed);
    let n = h.cfg.step_count;
    h.steps = h.cfg.base_steps + draw(&mut h, n) * h.cfg.step_len;
    h.dir = selection;
    w.entity_mut(e).insert(h);
}

/// Before the move: the velocity turns toward the delta; with no target the projectile falls.
fn steer(tg: Targets, clock: Res<crate::cow::Clock>, mut q: Query<(&mut Vel, &mut Homing, &crate::projectile::BornAt)>) {
    for (mut v, mut h, born) in &mut q {
        if born.0 == clock.0 {
            continue;
        }
        if tg.info(h.target).is_none() {
            v.0.y -= h.cfg.gravity;
            continue;
        }
        let (grow, max, turn) = (h.cfg.grow, h.cfg.max_delta, h.cfg.turn);
        h.delta = h.delta.map(|d| grow_delta(d, grow, max));
        v.0 = turn_velocity(v.0, h.delta, turn);
    }
}

/// After the move: the step count, the next direction when it runs out, and a block centre reached with a stand spot ahead.
fn follow(tg: Targets, clock: Res<crate::cow::Clock>, mut cmd: Commands, q: Query<(Entity, &Homing, &crate::projectile::BornAt)>) {
    for (e, h, born) in &q {
        if born.0 == clock.0 {
            continue;
        }
        if tg.info(h.target).is_none() {
            continue;
        }
        let h = *h;
        cmd.queue(move |w: &mut World| after_move(w, e, h));
    }
}
fn after_move(w: &mut World, e: Entity, h: Homing) {
    let Some(t) = targeting::info(w, h.target) else { return };
    if h.steps > 0 {
        let left = h.steps - 1;
        w.entity_mut(e).insert(Homing { steps: left, ..h });
        if left == 0 {
            select_next(w, e, h.dir.map(axis_of));
        }
    }
    let Some(h) = w.get::<Homing>(e).copied() else { return };
    let Some(d) = h.dir else { return };
    let pos = w.get::<Pos>(e).map(|p| p.0).unwrap_or(Vec3::ZERO);
    let p = cell(pos);
    let s = STEPS[d as usize];
    let axis = axis_of(d);
    let (lo, hi) = (t.bounds.min_position(), t.bounds.max_position());
    let c = [((lo.x + hi.x) / 2.0).floor() as i32, lo.y.floor() as i32, ((lo.z + hi.z) / 2.0).floor() as i32];
    let stand = [p[0] + s[0], p[1] + s[1], p[2] + s[2]];
    if block_stands(w, stand, pos.y) || p[axis as usize] == c[axis as usize] {
        select_next(w, e, Some(axis));
    }
}
/// The axis a direction moves along (the 3D data order: down, up, north, south, west, east).
pub fn axis_of(d: u8) -> u8 {
    match d {
        0 | 1 => 1,
        2 | 3 => 2,
        _ => 0,
    }
}
fn block_empty(w: &World, p: [i32; 3]) -> bool {
    crate::projectile::block_empty(w, p)
}
fn block_stands(w: &World, p: [i32; 3], y: f64) -> bool {
    crate::projectile::can_stand_on(w, p, y)
}
