//! Calculs de la tête du dragon (`Scanning`, D:2290-2330) : l'écart entre son cap et un point, l'accélération du lacet pour s'y
//! tourner, et le vecteur de vue (`view_vector` de Horizon, `game.rs`).
use horizon_core::geometry::{trig, Vec3};

/// `Util`-free `Mth.wrapDegrees` in doubles.
fn wrap64(d: f64) -> f64 {
    let mut r = d % 360.0;
    if r >= 180.0 {
        r -= 360.0;
    }
    if r < -180.0 {
        r += 360.0;
    }
    r
}

/// `Vec3.normalize` on doubles: zero below the length 1e-5.
fn unit3(x: f64, y: f64, z: f64) -> [f64; 3] {
    let len = (x * x + y * y + z * z).sqrt();
    if len < f64::from(1.0e-5_f32) { [0.0; 3] } else { [x / len, y / len, z / len] }
}

/// `view_vector(yaw, pitch)` of Horizon's `game.rs`: plain `f64` trigonometry.
pub fn view_vector(yaw: f32, pitch: f32) -> Vec3 {
    let (y, p) = (f64::from(yaw).to_radians(), f64::from(pitch).to_radians());
    Vec3::new(-y.sin() * p.cos(), -p.sin(), y.cos() * p.cos())
}

/// The angle in degrees, plus 0.5, between the heading `yaw` of a dragon at `pos` and the flat direction to `pp`.
pub fn off_heading(yaw: f32, pos: Vec3, pp: Vec3) -> f32 {
    let aim = unit3(pp.x - pos.x, 0.0, pp.z - pos.z);
    let r = f64::from(yaw * trig::DEG_TO_RAD);
    let dir = unit3(f64::from(trig::sin(r)), 0.0, f64::from(-trig::cos(r)));
    let dot = (dir[0] * aim[0] + dir[1] * aim[1] + dir[2] * aim[2]) as f32;
    (f64::from(dot).acos() * 180.0 / f64::from(std::f32::consts::PI)) as f32 + 0.5
}

/// The yaw acceleration after a turn toward the head offset `(xd, zd)`: the old acceleration decays to 0.8, and the clamped
/// wrapped difference between the yaw and the bearing pushes it, weighted by the distance.
pub fn turn_accel(yaw: f32, accel: f32, xd: f64, zd: f64) -> f32 {
    let delta = wrap64(180.0 - trig::atan2(xd, zd) * 180.0 / f64::from(std::f32::consts::PI) - f64::from(yaw)).clamp(-100.0, 100.0);
    let mut accel = accel;
    accel *= 0.8;
    let dist = ((xd * xd + zd * zd).sqrt() as f32 + 1.0).min(40.0);
    accel += delta as f32 * (0.7 / dist / dist);
    accel
}
