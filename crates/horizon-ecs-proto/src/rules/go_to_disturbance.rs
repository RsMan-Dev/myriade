//! Geometry of `go_to_disturbance`: the spot of the last vibration (its block centre) and the arrival test (2 blocks).

/// Squared distance under which the entity has arrived (`closerThan(block, 2)`).
const CLOSE_SQR: f64 = 4.0;

/// The centre of the block the disturbance was felt at.
pub fn spot_of(block: [i32; 3]) -> [f64; 3] {
    [f64::from(block[0]) + 0.5, f64::from(block[1]), f64::from(block[2]) + 0.5]
}

/// The entity at `at` is still too far from the disturbance block to have arrived.
pub fn still_away(block: [i32; 3], at: [f64; 3]) -> bool {
    let d = [f64::from(block[0]) + 0.5 - at[0], f64::from(block[1]) - at[1], f64::from(block[2]) + 0.5 - at[2]];
    d[0] * d[0] + d[1] * d[1] + d[2] * d[2] >= CLOSE_SQR
}
