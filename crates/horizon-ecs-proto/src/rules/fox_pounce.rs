//! The leap: the unit push toward the prey, the test that keeps a fox leaping, and the head's tilt of one tick.

/// The unit direction of `(dx, dy, dz)` in the horizontal plane (zero when the prey is at the feet).
pub fn unit_xz(dx: f64, dy: f64, dz: f64) -> (f64, f64) {
    let len = (dx * dx + dy * dy + dz * dz).sqrt();
    if len > 0.0 { (dx / len, dz / len) } else { (0.0, 0.0) }
}

/// The leap goes on while it moves up or down fast enough, is tilted, or is in the air.
pub fn keeps_flying(vy: f64, tilt: f32, on_ground: bool) -> bool {
    vy * vy >= f64::from(0.05f32) || tilt.abs() >= 15.0 || !on_ground
}

/// The head's tilt of one tick, from the pitch, the velocity `v` and whether the fox jumps.
pub fn next_tilt(pitch: f32, v: [f64; 3], jumping: bool) -> f32 {
    let [vx, vy, vz] = v;
    if vy * vy < f64::from(0.03f32) && pitch != 0.0 {
        return pitch + 0.2 * wrap_degrees(-pitch);
    }
    let horizontal = (vx * vx + vz * vz).sqrt();
    let bias = if jumping && vy > 0.0 { 6.5 } else { 1.0 };
    let y = vy * bias;
    let len = (horizontal * horizontal + y * y).sqrt();
    if len > f64::from(1.0e-5f32) {
        let angle = signum(-y) * (horizontal / len).acos() * 180.0 / f64::from(std::f32::consts::PI);
        return ((angle as f32) % 360.0).clamp(-90.0, 90.0);
    }
    pitch
}

/// `Mth.wrapDegrees` as the goals' helper.
fn wrap_degrees(a: f32) -> f32 {
    let mut a = if a.abs() < 360.0 { a } else { a % 360.0 };
    if a >= 180.0 {
        a -= 360.0;
    }
    if a < -180.0 {
        a += 360.0;
    }
    a
}

fn signum(x: f64) -> f64 {
    if x > 0.0 {
        1.0
    } else if x < 0.0 {
        -1.0
    } else {
        0.0
    }
}
