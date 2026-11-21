use horizon_core::geometry::Vec3;

/// The point of side head `nth` (0 or 1): `side` blocks out from the body at `yaw`, a half-turn apart, at `height`.
pub fn side_point(o: Vec3, yaw: f32, nth: u8, side: f64, height: f64) -> [f64; 3] {
    let a = f64::from((yaw + 180.0 * f32::from(nth)).to_radians());
    [o.x + a.cos() * side, o.y + height, o.z + a.sin() * side]
}

/// The velocity of a skull: 0.1 blocks per tick from `from` toward `at`.
pub fn shot_velocity(from: [f64; 3], at: [f64; 3]) -> Vec3 {
    let d = [at[0] - from[0], at[1] - from[1], at[2] - from[2]];
    let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt().max(1.0e-9);
    Vec3::new(d[0] / len * 0.1, d[1] / len * 0.1, d[2] / len * 0.1)
}

/// The aim point of a target: halfway up its body.
pub fn aim_point(pos: Vec3, eye: f64) -> [f64; 3] {
    [pos.x, pos.y + (eye - pos.y) * 0.5, pos.z]
}

/// The squared distance `d` is within `range`.
pub fn in_range_sq(d: f64, range: f64) -> bool {
    d <= range * range
}

/// The offset `(dx, dy, dz)` is within the half-sizes of the box `b`.
pub fn in_box(dx: f64, dy: f64, dz: f64, b: [f64; 3]) -> bool {
    dx.abs() <= b[0] && dy.abs() <= b[1] && dz.abs() <= b[2]
}

/// A uniform draw `unit` in [0, 1) spread over [-half, half).
pub fn spread(unit: f64, half: f64) -> f64 {
    unit * 2.0 * half - half
}
