//! The number of re-asks until a stay is picked, from one uniform draw.

/// The re-asks until the stay is picked, from one uniform draw `u` in [0, 1): geometric of `chance` (under 1), 1 at least.
pub fn asks_from_uniform(u: f64, chance: f64) -> i32 {
    let u = u.max(1.0e-12);
    (1 + (u.ln() / (1.0 - chance.max(1.0e-6)).ln()).floor() as i64).min(4000) as i32
}
