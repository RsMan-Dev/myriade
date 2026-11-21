//! The tempt's reach, calm-down and selector cadence: the distances that hold a player, and the clock of the calm-down.

/// The reach of a tempting player, in blocks: the range, at least 2.
pub fn tempt_reach(range: f64) -> f64 {
    range.max(2.0)
}

/// Whether the squared distance `d2` is within `reach` blocks.
pub fn in_reach(d2: f64, reach: f64) -> bool {
    d2 <= reach * reach
}

/// Whether the squared distance `d2` is under `stop` blocks: the animal stops walking.
pub fn within_stop(d2: f64, stop: f64) -> bool {
    d2 < stop * stop
}

/// Whether the calm-down is over at `now`.
pub fn calm_over(now: u64, calm_until: u64) -> bool {
    now >= calm_until
}

/// The end of the calm-down set when the goal stops: `now + calm`.
pub fn calm_end(now: u64, calm: u64) -> u64 {
    now + calm
}

/// The end of the calm-down moved by the time the goal was paused: `calm_until + (now - paused_at)`.
pub fn calm_resumed(calm_until: u64, now: u64, paused_at: u64) -> u64 {
    calm_until + (now - paused_at)
}

/// The ticks left of the calm-down at `now` (0 once it is over).
pub fn calm_left(calm_until: u64, now: u64) -> u64 {
    calm_until.saturating_sub(now)
}

/// Whether the selector asks the tempt this tick: every other tick, shifted by `phase`, from the second tick on.
pub fn runs_this_tick(now: u64, phase: u64) -> bool {
    !((now + phase) % 2 == 1 && now > 1)
}
