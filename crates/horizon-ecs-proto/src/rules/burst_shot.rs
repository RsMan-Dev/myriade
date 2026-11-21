//! Calculations of a burst shot: the spread of the aim, which grows with the square root of the distance.

/// The spread of the aim at squared distance `d2`: `spread * sqrt(sqrt(d2)) * 0.5` on each horizontal axis.
pub fn deviation(spread: f64, d2: f64) -> f64 {
    spread * (d2.sqrt().sqrt() * 0.5)
}

/// One horizontal coordinate of the aim: `mode` moved by the triangular spread of `dev` drawn from `a` and `b`.
pub fn spread_aim(mode: f64, dev: f64, a: f64, b: f64) -> f64 {
    mode + dev * (a - b)
}
