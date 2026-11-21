//! The numbers of a hook's waits (`FishingHook.catchingFish`): the lure's wait, the fish's approach, the approach's distance and
//! the ticks a counter takes at a fish's speed.
use crate::rules::fishing_hook::lure_wait;
use horizon_core::random::{LegacyRandom, RandomSource};

/// The lure's wait in the fish's ticks (`lureWait`): 100 to 600 less the rod's Lure.
pub fn lured_after(rng: &mut LegacyRandom, lure: i32) -> i32 {
    lure_wait(rng, lure)
}

/// The fish's approach in the fish's ticks (`Mth.nextInt(20, 80)`).
pub fn hooked_after(rng: &mut LegacyRandom) -> i32 {
    20 + rng.next_int_bound(61).unwrap_or(0)
}

/// The distance of the approach while `left` fish-ticks remain (`timeUntilHooked * 0.1`).
pub fn approach_distance(left: i32) -> f32 {
    left as f32 * 0.1
}

/// The world ticks a counter of `left` takes at `speed` per tick: `ceil(left / speed)`, at least one; none at speed 0 (it stands still).
pub fn ticks_at(left: i32, speed: i32) -> Option<u64> {
    if speed <= 0 {
        return None;
    }
    Some(((left.max(1) + speed - 1) / speed) as u64)
}

/// The tick of the next success of a chance `p` a tick, from the tick after now (geometric law): `ceil(ln(1 - u) / ln(1 - p))`, at least 1.
pub fn geometric_after(rng: &mut LegacyRandom, p: f32) -> u64 {
    let u = f64::from(rng.next_float());
    let p = f64::from(p).min(0.999_999);
    ((1.0 - u).ln() / (1.0 - p).ln()).ceil().max(1.0) as u64
}
