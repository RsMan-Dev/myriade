//! The launch of a ballistic arc (`LongJumpUtil.calculateJumpVectorForAngle`) and the shuffle of the angles drawn for it.

/// The squared launch speed `v0sq` and the speed `v0` for a target `r` away (`r2 = r * r`) and `y` higher, under `gravity`, at the angle
/// whose `[sin 2a, cos^2 a]` are given; `None` when no arc reaches it or the speed exceeds `max_velocity`.
pub fn launch_speed(r2: f64, r: f64, y: f64, gravity: f64, sin2: f64, cos_sq: f64, max_velocity: f32) -> Option<(f64, f64)> {
    let v0sq = r2 * gravity / (r * sin2 - 2.0 * y * cos_sq);
    if v0sq < 0.0 {
        return None;
    }
    let v0 = v0sq.sqrt();
    if v0 > f64::from(max_velocity) {
        return None;
    }
    Some((v0sq, v0))
}

/// `Util.shuffle` of the angles (`RandomSource.nextInt(i)` for i from the length down to 2), `draw(i)` being that draw.
pub fn shuffle<const N: usize>(angles: [i32; N], mut draw: impl FnMut(i32) -> i32) -> [i32; N] {
    let mut angles = angles;
    for i in (2..=N).rev() {
        let j = draw(i as i32) as usize;
        angles.swap(i - 1, j);
    }
    angles
}
