//! The warden's look: the range within which it follows its target, the point it looks at, and the disturbance's expiry.
use horizon_core::geometry::Vec3;

/// The squared range within which the warden follows its target with its head (24 blocks).
pub const FOLLOW_RANGE_SQR: f64 = 24.0 * 24.0;

/// Whether a squared distance is within the follow range.
pub fn in_follow_range(d2: f64) -> bool {
    d2 <= FOLLOW_RANGE_SQR
}

/// The point the head looks at for a target at `pos` whose eyes are `eye` above it.
pub fn eye_point(pos: Vec3, eye: f64) -> [f64; 3] {
    [pos.x, pos.y + eye, pos.z]
}

/// The centre of the block `p`.
pub fn block_centre(p: [i32; 3]) -> [f64; 3] {
    [f64::from(p[0]) + 0.5, f64::from(p[1]) + 0.5, f64::from(p[2]) + 0.5]
}

/// Whether the disturbance is still kept at `now` (its expiry is `until`).
pub fn still_disturbed(now: u64, until: u64) -> bool {
    now < until
}
