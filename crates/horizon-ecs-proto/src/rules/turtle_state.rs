//! The turtle's home distance: whether a position is nearer than `distance` to the centre of its home block.

/// Whether the position `p` is nearer than `distance` to the centre of the home block `home` (`closerToCenterThan`).
pub fn near_home_centre(home: [i32; 3], p: [f64; 3], distance: f64) -> bool {
    let d = [f64::from(home[0]) + 0.5 - p[0], f64::from(home[1]) + 0.5 - p[1], f64::from(home[2]) + 0.5 - p[2]];
    d[0] * d[0] + d[1] * d[1] + d[2] * d[2] < distance * distance
}
