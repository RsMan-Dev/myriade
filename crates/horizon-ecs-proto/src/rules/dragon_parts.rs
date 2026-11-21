//! The rules of the dragon's parts: the box of one part, the wrap of an angle, and where the eight parts stand from the dragon's
//! position, yaw and flight history (`place_dragon_parts`, D:2856). `s` holds the samples 0, 5 and 10 ticks back, `tail` 12, 14 and 16.
use horizon_core::geometry::{trig, Aabb, Vec3};

/// The box of a part at `pos` of size `(w, h)`: centred on its position, standing on it.
pub fn box_of(pos: [f64; 3], (w, h): (f64, f64)) -> Aabb {
    let p = pos;
    Aabb::new(Vec3::new(p[0] - w / 2.0, p[1], p[2] - w / 2.0), Vec3::new(p[0] + w / 2.0, p[1] + h, p[2] + w / 2.0))
}

/// `Mth.wrapDegrees(float)` as Horizon writes it (D:1092).
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

/// The eight parts' positions of a dragon at `p`, with `yaw`, yaw acceleration `accel` and `sitting`, step for step in the
/// `f32`/`f64` mix of Horizon's `place_dragon_parts`.
pub fn place_points(p: [f64; 3], yaw: f32, accel: f32, sitting: bool, s: [(f64, f32); 3], tail: [(f64, f32); 3]) -> [[f64; 3]; 8] {
    let (s0, s5, s10) = (s[0], s[1], s[2]);
    let (msin, mcos) = (|a: f32| trig::sin(f64::from(a)), |a: f32| trig::cos(f64::from(a)));
    let mut out = [[0.0_f64; 3]; 8];
    let mut set = |i: usize, x: f32, y: f64, z: f32| out[i] = [p[0] + f64::from(x), p[1] + y, p[2] + f64::from(z)];

    let tilt = (s5.0 - s10.0) as f32 * 10.0 * trig::DEG_TO_RAD;
    let (ct, st) = (mcos(tilt), msin(tilt));
    let r1 = yaw * trig::DEG_TO_RAD;
    let (s1, c1) = (msin(r1), mcos(r1));
    set(2, s1 * 0.5, 0.0, -c1 * 0.5);
    set(6, c1 * 4.5, 2.0, s1 * 4.5);
    set(7, c1 * -4.5, 2.0, s1 * -4.5);

    let r2 = yaw * trig::DEG_TO_RAD - accel * 0.01;
    let (s2, c2) = (msin(r2), mcos(r2));
    let y_off = if sitting { -1.0 } else { s5.0 - s0.0 } as f32;
    set(0, s2 * 6.5 * ct, f64::from(y_off + st * 6.5), -c2 * 6.5 * ct);
    set(1, s2 * 5.5 * ct, f64::from(y_off + st * 5.5), -c2 * 5.5 * ct);

    for (i, p0) in tail.iter().enumerate() {
        let rot = yaw * trig::DEG_TO_RAD + wrap(p0.1 - s5.1) * trig::DEG_TO_RAD;
        let (ss, cc) = (msin(rot), mcos(rot));
        let dd = (i as f32 + 1.0) * 2.0;
        let x = -(s1 * 1.5 + ss * dd) * ct;
        let y = p0.0 - s5.0 - f64::from((dd + 1.5) * st) + 1.5;
        let z = (c1 * 1.5 + cc * dd) * ct;
        set(3 + i, x, y, z);
    }
    out
}
