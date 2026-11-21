//! The rules of the despawn of a creature: whether it may go, when its chance of going comes up, and whether it is too far from the players.

/// Whether a creature that no player is near may go. The rule of its species, not of the system.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Removable {
    Always,
    /// Animals, villagers, golems...
    Never,
    /// `Cat.removeWhenFarAway`: while untamed and older than this many ticks.
    UntamedAfter(u32),
}

/// Whether a creature may go, given its rule, whether it is tamed and its age in ticks.
pub fn removable(rule: Removable, tamed: bool, age: u64) -> bool {
    match rule {
        Removable::Always => true,
        Removable::Never => false,
        Removable::UntamedAfter(t) => !tamed && age > u64::from(t),
    }
}

/// The own-time tick at which the chance of going comes up: `idle + 1` ticks from `now`, then the first success `drawn` (never negative).
pub fn deadline(now: u64, idle: u32, drawn: i32) -> u64 {
    now + u64::from(idle) + 1 + drawn.max(0) as u64
}

/// Whether the squared distance to the nearest player is inside the ring of `near` blocks.
pub fn within_ring(nearest_sqr: f64, near: f32) -> bool {
    nearest_sqr < f64::from(near).powi(2)
}

/// Whether the point `(x, z)` is past `distance` blocks from every player (`players`: their `(x, z)`). Without a player, it is not.
pub fn is_far(x: f64, z: f64, players: &[(f64, f64)], distance: f32) -> bool {
    players.iter().map(|p| (p.0 - x).powi(2) + (p.1 - z).powi(2)).reduce(f64::min).is_some_and(|best| best > f64::from(distance).powi(2))
}
