//! `LongJumpUtil` as a service (the frog's and the goat's goal `behaviours/long_jump.rs`, the breeze's behaviour
//! `brain/behaviours/breeze.rs`): the velocity of the ballistic arc from a position to a target at a given launch angle, optionally with the
//! arc's collision test (`isClearTransition`: the jumper's box fits at points no further apart than nine tenths of its smaller side), and the
//! choice among angles drawn in a shuffled order. The species give the angles, the maximum velocity and whether the arc is tested.
use myriade_engine_lib::mobs::MobWorld;
use horizon_core::geometry::{trig, Aabb, Vec3};
use crate::rules::jump_arc::launch_speed;
use horizon_core::jmath;
pub use crate::rules::jump_arc::shuffle;

/// The jumper's box for the arc's collision test.
#[derive(Clone, Copy)]
pub struct Footprint {
    pub width: f32,
    pub height: f32,
    /// The world's occupancy masks, when the caller has them (the same answer as the kernel's, for less).
    pub occ: Option<&'static crate::occupancy::Occupancy>,
}

/// `LongJumpUtil.calculateJumpVectorForAngle`: from `pos` to `target` (stopping half a block short of it) at `angle` degrees, under `gravity`;
/// `None` when the speed needed exceeds `max_velocity` or, with a `collide` world and box, when the arc meets a block.
pub fn jump_vector(pos: Vec3, gravity: f64, target: Vec3, max_velocity: f32, angle: i32, collide: Option<(&MobWorld, Footprint)>) -> Option<Vec3> {
    let plane = Vec3::new(target.x - pos.x, 0.0, target.z - pos.z).normalize().scale(0.5);
    let aim = target.subtract(plane);
    let d = aim.subtract(pos);
    let angrad = angle as f32 * std::f32::consts::PI / 180.0;
    // Math.atan2 is StrictMath.atan2 (fdlibm).
    let xz = trig::fdlibm_atan2(d.z, d.x);
    let r2 = d.x * d.x + 0.0 + d.z * d.z;
    let r = r2.sqrt();
    let y = d.y;
    let g = gravity;
    let [sin2, cos_sq, sin, cos] = angle_trig(angle, angrad);
    let (v0sq, v0) = launch_speed(r2, r, y, g, sin2, cos_sq, max_velocity)?;
    let (v0r, v0y) = (v0 * cos, v0 * sin);
    if let Some((mw, foot)) = collide {
        // `ceil(r / v0r) * 2` points along the arc, each step clear of blocks.
        let samples = (r / v0r).ceil() as i32 * 2;
        let (sin_xz, cos_xz) = (jmath::sin(xz), jmath::cos(xz));
        let mut ri = 0.0;
        let mut previous: Option<Vec3> = None;
        for _ in 0..samples - 1 {
            ri += r / f64::from(samples);
            let yi = sin / cos * ri - jmath::pow(ri, 2.0) * g / (2.0 * v0sq * jmath::pow(cos, 2.0));
            let p = Vec3::new(pos.x + ri * cos_xz, pos.y + yi, pos.z + ri * sin_xz);
            if previous.is_some_and(|q| !clear_transition(mw, foot, q, p)) {
                return None;
            }
            previous = Some(p);
        }
    }
    Some(Vec3::new(v0r * jmath::cos(xz), v0y, v0r * jmath::sin(xz)).scale(f64::from(0.95_f32)))
}

thread_local! {
    static ANGLES: std::cell::RefCell<Vec<(i32, [f64; 4])>> = const { std::cell::RefCell::new(Vec::new()) };
}
/// `[sin 2a, cos^2 a, sin a, cos a]` of an allowed angle: the same four angles for every candidate, worked out once.
fn angle_trig(angle: i32, angrad: f32) -> [f64; 4] {
    ANGLES.with(|c| {
        let mut c = c.borrow_mut();
        if let Some(e) = c.iter().find(|e| e.0 == angle) {
            return e.1;
        }
        let v = [jmath::sin(f64::from(2.0 * angrad)), jmath::pow(jmath::cos(f64::from(angrad)), 2.0), jmath::sin(f64::from(angrad)), jmath::cos(f64::from(angrad))];
        c.push((angle, v));
        v
    })
}

/// `LongJumpUtil.isClearTransition`.
fn clear_transition(mw: &MobWorld, foot: Footprint, from: Vec3, to: Vec3) -> bool {
    let (width, height) = (f64::from(foot.width), f64::from(foot.height));
    let d = to.subtract(from);
    let min = width.min(height);
    let checks = (d.length() / min).ceil() as i32;
    let step = d.normalize().scale(min * f64::from(0.9_f32));
    let half = width / 2.0;
    let mut p = from;
    (0..checks).all(|i| {
        p = if i == checks - 1 { to } else { p.add_vector(step) };
        let b = Aabb::new(Vec3::new(p.x - half, p.y, p.z - half), Vec3::new(p.x + half, p.y + height, p.z + half));
        match foot.occ {
            Some(o) => o.no_collision(myriade_level::collisions::SceneCollisions { world: mw.world, binding: mw.collision, subject: None }, b),
            None => mw.no_collision(b),
        }
    })
}

/// The first of `angles` (already in the order drawn: `shuffle` does it with the entity's stream) whose arc reaches the target.
pub fn first_reaching(angles: &[i32], pos: Vec3, gravity: f64, target: Vec3, max_velocity: f32, collide: Option<(&MobWorld, Footprint)>) -> Option<Vec3> {
    angles.iter().find_map(|&a| jump_vector(pos, gravity, target, max_velocity, a, collide))
}
