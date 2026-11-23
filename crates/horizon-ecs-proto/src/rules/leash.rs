//! The numbers of a lead: the other end's state, the elastic pull, the stretch, the walk to the holder and the spin the rope gives.
use horizon_core::geometry::{Aabb, Vec3};

/// Where the other end of the lead is this tick.
#[derive(Clone, Copy, Debug)]
pub struct HolderState {
    pub position: Vec3,
    pub center: Vec3,
    pub width: f64,
    pub height: f64,
    pub yaw: f32,
    pub velocity: Vec3,
}
/// The centre of a box.
pub fn center_of(b: Aabb) -> Vec3 {
    let (lo, hi) = (b.min_position(), b.max_position());
    Vec3::new((lo.x + hi.x) / 2.0, (lo.y + hi.y) / 2.0, (lo.z + hi.z) / 2.0)
}
/// `LeashFenceKnotEntity`: 0,375 wide, 0,5 tall, centred a quarter block above its base.
pub fn knot_state(p: [i32; 3]) -> HolderState {
    let position = Vec3::new(f64::from(p[0]) + 0.5, f64::from(p[1]) + 0.375, f64::from(p[2]) + 0.5);
    HolderState { position, center: Vec3::new(position.x, position.y + 0.25, position.z), width: 0.375, height: 0.5, yaw: 0.0, velocity: Vec3::ZERO }
}
/// `Vec3.yRot` with a float angle.
fn rotate(v: [f64; 3], a: f64) -> [f64; 3] {
    let r = Vec3::new(v[0], v[1], v[2]).rotate_y(a as f32);
    [r.x, r.y, r.z]
}
/// `Leashable.computeElasticInteraction` for the single attachment of a creature (0, 0,5, 0,5 of its box) and its holder (0, 0,5, 0): the
/// force toward the holder and the torque, or `None` while slack.
pub fn elastic_pull(pos: Vec3, yaw: f32, size: (f64, f64), holder: &HolderState, motion: Vec3, elastic: f64) -> Option<([f64; 3], f64)> {
    pull(pos, yaw, size, holder, motion, elastic, (&[[0.0, 0.5, 0.5]], &[[0.0, 0.5, 0.0]], 1.0))
}
/// `Leashable.checkElasticInteractions` with a quad lead: the four shared corners of each end pull, a quarter of the sum each.
pub fn quad_pull(pos: Vec3, yaw: f32, size: (f64, f64), holder: &HolderState, motion: Vec3, elastic: f64) -> Option<([f64; 3], f64)> {
    const SHARED: [[f64; 3]; 4] = [[-0.5, 0.5, 0.5], [-0.5, 0.5, -0.5], [0.5, 0.5, -0.5], [0.5, 0.5, 0.5]];
    pull(pos, yaw, size, holder, motion, elastic, (&SHARED, &SHARED, 0.25))
}
/// `Leashable.createQuadLeashOffsets`: the four attachment points of a quad lead on a body of this size, front-left first, clockwise from above.
pub fn quad_points(width: f32, height: f32, front: f64, front_back: f64, left_right: f64, up: f64) -> [[f64; 3]; 4] {
    let (fb, lr) = (front_back * f64::from(width), left_right * f64::from(width));
    let (f, y) = (front * f64::from(width), up * f64::from(height));
    [[-lr, y, fb + f], [-lr, y, -fb + f], [lr, y, -fb + f], [lr, y, fb + f]]
}
/// The pull of each pair of attachment points (`own` on the creature, `theirs` on the holder), summed and scaled; `None` while every pair is slack.
fn pull(pos: Vec3, yaw: f32, size: (f64, f64), holder: &HolderState, motion: Vec3, elastic: f64, ends: (&[[f64; 3]], &[[f64; 3]], f64)) -> Option<([f64; 3], f64)> {
    let (own, theirs, scale) = ends;
    let (w, h) = size;
    let (mut force, mut torque, mut any) = ([0.0; 3], 0.0, false);
    for (mine, other) in own.iter().zip(theirs) {
        let lever = rotate([mine[0] * w, mine[1] * h, mine[2] * w], -f64::from(yaw.to_radians()));
        let object = [pos.x + lever[0], pos.y + lever[1], pos.z + lever[2]];
        let arm = rotate([other[0] * holder.width, other[1] * holder.height, other[2] * holder.width], -f64::from(holder.yaw.to_radians()));
        let pivot = [holder.position.x + arm[0], holder.position.y + arm[1], holder.position.z + arm[2]];
        let d = [pivot[0] - object[0], pivot[1] - object[1], pivot[2] - object[2]];
        let distance = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        if distance < elastic {
            continue;
        }
        let unit = if distance < 1.0e-5 { [0.0; 3] } else { [d[0] / distance, d[1] / distance, d[2] / distance] };
        let slack = distance - elastic;
        let mut f = [unit[0] * slack, unit[1] * slack, unit[2] * slack];
        let t = lever[2] * f[0] - lever[0] * f[2];
        if motion.x * f[0] + motion.y * f[1] + motion.z * f[2] >= 0.0 {
            for v in &mut f {
                *v *= f64::from(0.3_f32);
            }
        }
        for k in 0..3 {
            force[k] += f[k];
        }
        torque += t;
        any = true;
    }
    any.then(|| ([force[0] * scale, force[1] * scale, force[2] * scale], torque * scale))
}
/// Whether the lead is stretched: the distance to the holder's centre is past the elastic distance less both half-widths.
pub fn is_stretched(distance: f64, elastic: f64, holder_width: f64, body_width: f32) -> bool {
    distance > elastic - holder_width - f64::from(body_width)
}
/// The point a slack lead walks the creature to: two blocks short of the holder, on the line from `pos` to `holder`.
pub fn follow_target(pos: Vec3, holder: Vec3) -> [f64; 3] {
    let d = Vec3::new(holder.x - pos.x, holder.y - pos.y, holder.z - pos.z);
    let len = d.length();
    let scale = if len > 0.0 { (len - 2.0).max(0.0) / len } else { 0.0 };
    [pos.x + d.x * scale, pos.y + d.y * scale, pos.z + d.z * scale]
}
/// The friction the spin wears by: the ground's block friction times 0,91 on the ground, 0,8 in water, 0,91 in the air.
pub fn spin_friction(ground: Option<f32>, in_water: bool) -> f32 {
    match ground {
        Some(f) => f * 0.91_f32,
        None if in_water => 0.8_f32,
        None => 0.91_f32,
    }
}
/// The spin after one tick of friction: zero once it is below 1e-9.
pub fn spin_step(spin: f64, friction: f32) -> f64 {
    let s = spin * f64::from(friction);
    if s.abs() < 1.0e-9 { 0.0 } else { s }
}
