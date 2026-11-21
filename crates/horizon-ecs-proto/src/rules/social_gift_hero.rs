//! `GiveGiftToHero` (`brain/behaviours/social_gift_hero.rs`): the timers, the reach of a hero, the walk to it, and the throw of the gift.
use horizon_core::geometry::Vec3;

/// The wait before the first gift, from the first check (a deadline).
pub const GIFT_WAIT_TICKS: u64 = 600;
/// The time the gift is ready after a start.
pub const GIFT_READY_TICKS: u64 = 20;
/// The close-enough distance of the walk to the hero, in blocks.
pub const HERO_CLOSE: i32 = 5;
/// The walk's speed towards the hero.
pub const HERO_SPEED: f32 = 0.5;
/// The speed of a thrown gift, in blocks per tick.
pub const THROW_SPEED: f64 = 0.3;

/// Whether the blocks of `me` and `at` are within 5 blocks (25 squared, `BlockPos.closerThan`).
pub fn within_five(me: Vec3, at: Vec3) -> bool {
    let block = |v: Vec3| [v.x.floor() as i32, v.y.floor() as i32, v.z.floor() as i32];
    let (a, b) = (block(me), block(at));
    (0..3).map(|i| (a[i] - b[i]).pow(2)).sum::<i32>() < 25
}

/// `BehaviorUtils.throwItem`'s velocity: `THROW_SPEED` blocks per tick from `from` towards `at`.
pub fn throw_velocity(from: Vec3, at: Vec3) -> Vec3 {
    let d = Vec3::new(at.x - from.x, at.y - from.y, at.z - from.z);
    let len = (d.x * d.x + d.y * d.y + d.z * d.z).sqrt().max(1e-9);
    Vec3::new(d.x / len * THROW_SPEED, d.y / len * THROW_SPEED, d.z / len * THROW_SPEED)
}
