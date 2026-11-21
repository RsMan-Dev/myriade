//! The elder guardian's curse: the ticks to the next one, the test of a player's effect, and the range of the curse.

/// The ticks until `(now + id) % period == 0`: a period away when that tick is now, a tick away at least.
pub fn wait_to_curse(now: i64, id: i64, period: i64) -> i64 {
    let phase = (now + id).rem_euclid(period);
    if phase == 0 { period } else { period - phase }
}

/// A player under the effect takes the curse when its level is under the wanted one, or its time is nearly out.
pub fn takes_curse<A: PartialOrd>(have: A, want: A, left: u64, refresh: u64) -> bool {
    have < want || left <= refresh
}

/// The offset `(dx, dy, dz)` is within range: the squared distance is under the squared range.
pub fn in_curse_range(dx: f64, dy: f64, dz: f64, range: f64) -> bool {
    let d2 = dx.powi(2) + dy.powi(2) + dz.powi(2);
    d2 < range * range
}
