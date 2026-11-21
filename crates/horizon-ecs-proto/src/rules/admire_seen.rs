//! Admiration of a wanted item seen on the ground (`AdmireSeen`, `Memories::admire_seen`): reach, give-up and restart rules.
use horizon_core::geometry::{Aabb, Vec3};

pub const RANGE: f64 = 14.0;
pub const EYE: f64 = 1.62;
pub const SPEED: f64 = 1.0;

/// What a running goal does this tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Step {
    /// The goal ends: the item is gone or too far, or the time is over.
    End,
    /// The animal tired of trying: no goal before `walk_off_until`, then the goal ends.
    GiveUp { walk_off_until: u64 },
    /// The goal goes on: the animal tries once more.
    Go,
}

/// `step`: the tick of a running goal. `nearest_sqr` is the squared distance of the nearest wanted item in reach, if any.
pub fn step(now: u64, until: u64, reach: u32, tired: i32, walk_off: i32, nearest_sqr: Option<f64>, too_far_sqr: f64) -> Step {
    if !nearest_sqr.is_some_and(|d| d < too_far_sqr) {
        return Step::End;
    }
    if now >= until {
        return Step::End;
    }
    if reach > tired as u32 {
        return Step::GiveUp { walk_off_until: now + walk_off as u64 };
    }
    Step::Go
}

/// `canUse`: the off hand is empty and no walk-off is under way (`walk_off_until`, if one began).
pub fn can_start(off_empty: bool, now: u64, walk_off_until: Option<u64>) -> bool {
    off_empty && walk_off_until.is_none_or(|until| now >= until)
}

/// `start`: the tick the goal ends, `duration` ticks after `now`.
pub fn until(now: u64, duration: i32) -> u64 {
    now + duration as u64
}

/// `equip`: the tick a creation's `disabled` ticks end, if any were given.
pub fn disabled_until(now: u64, disabled: i32) -> Option<u64> {
    (disabled > 0).then(|| now + disabled as u64)
}

/// `after_change`: the ticks before the goal is asked again after it ended (at least 1).
pub fn wait_after_end(now: u64, walk_off_until: Option<u64>) -> i32 {
    walk_off_until.map_or(1, |until| until.saturating_sub(now).max(1)) as i32
}

/// The squared distance is within reach of the animal (the item search and the drop wake-up).
pub fn in_reach(dist_sqr: f64) -> bool {
    dist_sqr <= RANGE * RANGE
}

/// The box searched for wanted items around the animal's place.
pub fn search_area(at: Vec3) -> Aabb {
    Aabb::new(Vec3::new(at.x - RANGE, at.y - RANGE, at.z - RANGE), Vec3::new(at.x + RANGE, at.y + RANGE, at.z + RANGE))
}

/// The eye of the animal, from its place, for the sight of a wanted item.
pub fn eye_of(at: Vec3) -> Vec3 {
    Vec3::new(at.x, at.y + EYE, at.z)
}
