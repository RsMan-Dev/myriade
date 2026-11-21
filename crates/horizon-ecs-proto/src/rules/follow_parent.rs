//! The baby's follow of its parent: the search box around it, the distance that makes it leave, and the repath period.

/// The adult is in the search box: 8 blocks along x and z, 4 up or down.
pub fn near_adult(dx: f64, dy: f64, dz: f64) -> bool {
    dx <= 8.0 && dy <= 4.0 && dz <= 8.0
}

/// The baby is at least 3 blocks (squared 9) from the adult it follows.
pub fn beyond_nine(d2: f64) -> bool {
    d2 >= 9.0
}

/// A mother's moves wake the babies that are at least 3 blocks away from her (horizontal distance, squared).
pub fn apart(dx: f64, dz: f64) -> bool {
    dx.powi(2) + dz.powi(2) >= 9.0
}

/// An adult that moved within the 9-block box of a baby wakes it (each axis).
pub fn in_watch_box(dx: f64, dz: f64) -> bool {
    dx.abs() <= 9.0 && dz.abs() <= 9.0
}

/// The parent is still followed when it is 3 to 16 blocks away (squared 9..=256).
pub fn follows(d2: f64) -> bool {
    (9.0..=256.0).contains(&d2)
}

/// The next path to the parent is made 10 ticks after `now`.
pub fn next_repath(now: u64) -> u64 {
    now + 10
}
