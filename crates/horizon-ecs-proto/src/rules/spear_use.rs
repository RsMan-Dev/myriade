use horizon_core::geometry::Vec3;

/// The distance from `a` to the point `b`.
pub fn dist(a: Vec3, b: [f64; 3]) -> f64 {
    ((a.x - b[0]).powi(2) + (a.y - b[1]).powi(2) + (a.z - b[2]).powi(2)).sqrt()
}

/// The use duration `n` of the weapon, halved when `reduced` (the zombie goal's `reducedTickDelay`).
pub fn halved_use(n: i32, reduced: bool) -> i32 {
    if reduced { (n + 1) / 2 } else { n }
}

/// The charge speed and the distance bonus of a mount: the goal's values when `riding`, none otherwise.
pub fn mount_bonus(riding: bool, mount_speed: f64, mount_distance: i32) -> (f64, i32) {
    if riding { (mount_speed, mount_distance) } else { (1.0, 0) }
}

/// The band of distances from `lo` to `hi` blocks, plus the mount's `bonus`, from a target at distance `d`.
pub fn ring(lo: i32, hi: i32, bonus: i32, d: f64) -> (f64, f64) {
    (f64::from(lo + bonus) - d, f64::from(hi + bonus) - d)
}

/// The unit view direction from `eye` toward a target at `target` with eye height `target_eye`.
pub fn look_dir(eye: Vec3, target: Vec3, target_eye: f64) -> Vec3 {
    let to = Vec3::new(target.x - eye.x, target_eye - eye.y, target.z - eye.z);
    let len = (to.x * to.x + to.y * to.y + to.z * to.z).sqrt().max(1e-9);
    Vec3::new(to.x / len, to.y / len, to.z / len)
}

/// The distance band of a spot: its minimum is at least `floor` and its maximum at least `ceil`, both floored.
pub fn away_band(min: f64, max: f64, floor: f64, ceil: f64) -> (f64, f64) {
    (min.max(floor).floor(), max.max(ceil).floor())
}
