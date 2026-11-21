//! Calculs du vol : pousée vers le point visé, arrivée, cap et vitesse de la banque, traînée de l'air et des fluides.
use horizon_core::geometry::{trig::Mth, Vec3};

/// The `Vec3` distance below which a wanted point's offset is no motion (`abs > EPSILON` tests of the flier's pitch).
pub const EPSILON: f64 = 1.0e-5;

/// `GhastMoveControl`: a push of `FLYING_SPEED * 5 / 3` along the unit vector of `t` (length `len`).
pub fn float_push(v: Vec3, t: Vec3, len: f64) -> Vec3 {
    let speed = 0.06 * 5.0 / 3.0;
    v.add_vector(Vec3::new(t.x / len * speed, t.y / len * speed, t.z / len * speed))
}
/// The mean size of a box from its corners.
pub fn mean_size(lo: Vec3, hi: Vec3) -> f64 {
    ((hi.x - lo.x) + (hi.y - lo.y) + (hi.z - lo.z)) / 3.0
}
/// Accelerate straight at the wanted point: the offset `dv` of length `len`, scaled by the speed modifier.
pub fn aim_push(v: Vec3, dv: Vec3, speed_modifier: f64, len: f64) -> Vec3 {
    v.add_vector(dv.scale(speed_modifier * 0.05 / len))
}
/// The heading of a flier that faces its motion, in degrees.
pub fn face_motion(v: Vec3) -> f32 {
    -(v.x.atan2(v.z) as f32).to_degrees()
}
/// The velocity blends toward `speed` along `dv` (length `len`) by `accel`.
pub fn steer(u: Vec3, dv: Vec3, speed: f64, len: f64, accel: f64) -> Vec3 {
    let want = dv.scale(speed / len);
    Vec3::new(u.x + (want.x - u.x) * accel, u.y + (want.y - u.y) * accel, u.z + (want.z - u.z) * accel)
}
/// The vertical velocity of a flier at rest on the ground.
pub fn rest_vy(gravity: f64, vertical_drag: f32) -> f64 {
    (0.0 - gravity) * f64::from(vertical_drag)
}
/// `travelFlying`: the speed and the drag of the medium (0.91 in the air, 0.8 in water, 0.5 in lava).
pub fn free_speed_drag(in_water: bool, in_lava: bool, drive_air: bool, drive_speed: f32, air_speed: f32) -> (f32, f64) {
    if in_water {
        (0.02_f32, f64::from(0.8_f32))
    } else if in_lava {
        (0.02_f32, 0.5)
    } else {
        (if drive_air { drive_speed } else { air_speed }, f64::from(0.91_f32))
    }
}
/// `travelFlying` after the move: the velocity times the medium's drag.
pub fn drag_velocity(m: Vec3, drag: f64) -> Vec3 {
    Vec3::new(m.x * drag, m.y * drag, m.z * drag)
}
/// `noPhysics`: the air's friction after a move through the blocks.
pub fn noclip_drag(u: Vec3) -> Vec3 {
    Vec3::new(u.x * 0.91, u.y * 0.98, u.z * 0.91)
}
/// `travelInAir` of a flier after the move: the friction `friction` (0.6 on the ground at the start, else 1.0, times 0.91), the vertical drag, and the gravity.
pub fn air_velocity(m: Vec3, friction: f32, vertical_drag: f32, gravity: f64) -> Vec3 {
    let f = friction * 0.91_f32;
    Vec3::new(m.x * f64::from(f), (m.y - gravity) * f64::from(vertical_drag), m.z * f64::from(f))
}
/// `PhantomMoveControl`: the bank toward the offset `(dx, dy, dz)` from heading `yaw` at `speed`, the velocity `u` blending toward it.
/// Returns the new heading, speed and velocity; unchanged when the point is straight above or below.
pub fn bank(yaw: f32, speed: f32, dx: f64, dy: f64, dz: f64, u: Vec3) -> (f32, f32, Vec3) {
    let (mut dx, mut dz) = (dx, dz);
    let mut sd = (dx * dx + dz * dz).sqrt();
    if sd.abs() <= 1.0e-5 {
        return (yaw, speed, u);
    }
    let scale = 1.0 - (dy * 0.7).abs() / sd;
    dx *= scale;
    dz *= scale;
    sd = (dx * dx + dz * dz).sqrt();
    let sd2 = (dx * dx + dz * dz + dy * dy).sqrt();
    let prev = yaw;
    let angle = dz.atan2(dx) as f32;
    let a = wrap_degrees(yaw + 90.0);
    let bb = wrap_degrees(angle.to_degrees());
    let diff = wrap_degrees(bb - a);
    let yaw = a + diff.clamp(-4.0, 4.0) - 90.0;
    let speed = if wrap_degrees(prev - yaw).abs() < 3.0 { approach(speed, 1.8, 0.005 * (1.8 / speed)) } else { approach(speed, 0.2, 0.025) };
    let x_rot = -((-dy).atan2(sd) as f32).to_degrees();
    let move_angle = (yaw + 90.0).to_radians();
    let tx = f64::from(speed * move_angle.mcos()) * (dx / sd2).abs();
    let tz = f64::from(speed * move_angle.msin()) * (dz / sd2).abs();
    let ty = f64::from(speed * x_rot.to_radians().msin()) * (dy / sd2).abs();
    (yaw, speed, Vec3::new(u.x + (tx - u.x) * 0.2, u.y + (ty - u.y) * 0.2, u.z + (tz - u.z) * 0.2))
}
/// Wraps an angle in degrees to [-180, 180).
pub fn wrap_degrees(d: f32) -> f32 {
    let r = d.rem_euclid(360.0);
    if r >= 180.0 { r - 360.0 } else { r }
}
/// Moves `from` toward `to` by `step`, without overshooting.
pub fn approach(from: f32, to: f32, step: f32) -> f32 {
    if from < to { (from + step).min(to) } else { (from - step).max(to) }
}
