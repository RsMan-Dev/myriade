//! `SocialiseAtBell` (`brain/behaviours/social_socialize_bell.rs`): the reach of the bell, the villager reach, and the walk's pace.
use horizon_core::geometry::Vec3;

/// The walk's speed towards the chosen villager.
pub const SPEED: f32 = 0.3;
/// The walk's close-enough distance in blocks.
pub const CLOSE: i32 = 1;

/// `closerToCenterThan(bell, 4)`: the centre of the bell block at `b` is within 4 blocks of `pos` (16 squared).
pub fn at_bell(b: [i32; 3], pos: Vec3) -> bool {
    let c = Vec3::new(f64::from(b[0]) + 0.5, f64::from(b[1]) + 0.5, f64::from(b[2]) + 0.5);
    c.distance_squared(pos) < 16.0
}

/// Whether a villager at squared distance `dist2` can be chosen: 32 squared or less.
pub fn in_reach(dist2: f64) -> bool {
    dist2 <= 32.0
}
