//! Calculs du vol du dragon (`EnderDragon.aiStep`, `Dragon`, D:2646-2856) : pas de battement d'aile, direction vers la cible de
//! la phase, glissement. Aucun tirage ici : le rng reste dans le système, dans l'ordre de Horizon.
use horizon_core::geometry::{trig, Vec3};

/// `Mth.wrapDegrees(float)`.
pub fn wrap(d: f32) -> f32 {
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
fn unit(v: [f64; 3]) -> [f64; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len < f64::from(1.0e-5_f32) {
        [0.0; 3]
    } else {
        [v[0] / len, v[1] / len, v[2] / len]
    }
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// What the phase asks of the steering this tick: the fly target (none: no steering), the top speed, whether the landing's turn
/// applies, and whether the wing is inside a wall (the move is shortened).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Steer {
    pub target: Option<Vec3>,
    pub speed: f32,
    pub landing: bool,
    pub in_wall: bool,
}

/// The flap's step of this tick: a steady 0.1 while sitting, half the flight's flap speed in a wall, the flight's flap speed
/// otherwise (`flap_speed` from the horizontal speed and the vertical one).
pub fn flap_increment(vel: Vec3, sitting: bool, in_wall: bool) -> f32 {
    if sitting {
        return 0.1;
    }
    let mut speed = 0.2 / (((vel.x * vel.x + vel.z * vel.z).sqrt() as f32) * 10.0 + 1.0);
    speed *= 2f64.powf(vel.y) as f32;
    if in_wall { speed * 0.5 } else { speed }
}

/// After the phases: the steering toward `s.target`, the move, and the slide. `in_wall` shortens the move (0.8).
pub fn steer(s: Steer, pos: &mut Vec3, vel: &mut Vec3, yaw: &mut f32, yaw_accel: &mut f32) {
    let Some(t) = s.target else { return };
    let (xd, mut yd, zd) = (t.x - pos.x, t.y - pos.y, t.z - pos.z);
    let dist2 = xd * xd + yd * yd + zd * zd;
    let max = f64::from(s.speed);
    let horizontal = (xd * xd + zd * zd).sqrt();
    if horizontal > 0.0 {
        yd = (yd / horizontal).clamp(-max, max);
    }
    vel.y += yd * 0.01;
    *yaw = wrap(*yaw);
    let aim = unit([xd, t.y - pos.y, zd]);
    let r = f64::from(*yaw * trig::DEG_TO_RAD);
    let dir = unit([f64::from(trig::sin(r)), vel.y, f64::from(-trig::cos(r))]);
    let facing = ((dot(dir, aim) as f32 + 0.5) / 1.5).max(0.0);
    if xd.abs() > f64::from(1.0e-5_f32) || zd.abs() > f64::from(1.0e-5_f32) {
        let rot = (vel.x * vel.x + vel.z * vel.z).sqrt() as f32 + 1.0;
        let turn = if s.landing { rot.min(40.0) / rot } else { 0.7 / rot.min(40.0) / rot };
        let delta = wrap(180.0 - trig::atan2(xd, zd) as f32 * trig::RAD_TO_DEG - *yaw).clamp(-50.0, 50.0);
        *yaw_accel *= 0.8;
        *yaw_accel += delta * turn;
        *yaw += *yaw_accel * 0.1;
    }
    let span = (2.0 / (dist2 + 1.0)) as f32;
    let speed = 0.06 * (facing * span + (1.0 - span));
    let r = f64::from(*yaw * trig::DEG_TO_RAD);
    vel.x += f64::from(speed) * f64::from(trig::sin(r));
    vel.z += -f64::from(speed) * f64::from(trig::cos(r));
    let step = if s.in_wall { 0.800000011920929 } else { 1.0 };
    // The move goes through the level's move gate in Horizon; here the position takes the step as it is.
    pos.x += vel.x * step;
    pos.y += vel.y * step;
    pos.z += vel.z * step;
    let actual = unit([vel.x, vel.y, vel.z]);
    let slide = 0.8 + 0.15 * (dot(actual, dir) + 1.0) / 2.0;
    vel.x *= slide;
    vel.y *= 0.9100000262260437;
    vel.z *= slide;
}
