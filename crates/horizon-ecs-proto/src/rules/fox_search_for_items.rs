//! The fox's item search: the memory of a hurt, the stop of a walk to an item, and the odds of the draws that end it.

/// `LastHurtBy`: the memory of the last attacker lasts 100 ticks after the hurt `at`.
pub fn hurt_memory_open(now: u64, at: u64) -> bool {
    now <= at + 100
}
/// The own time the memory of a hurt ends: 101 ticks after `at`.
pub fn hurt_memory_end(at: u64) -> u64 {
    at + 101
}
/// The wait of the hurt memory's end: at least one tick from now to `until`.
pub fn hurt_wait(until: u64, now: u64) -> i32 {
    (until - now).max(1) as i32
}
/// `Fox.FoxSearchForItemsGoal.start`: the stop of the walk after `k` continues, `2 x (k + 1)` ticks from `now`.
pub fn search_until(now: u64, k: u64) -> u64 {
    now + 2 * (k + 1)
}
/// The continues drawn from `draw` in [0, 1): the number of times a 1-in-5 continue happens, `floor(ln u / ln 0.2)`.
pub fn continues(draw: f64) -> u64 {
    let u = draw.max(1.0e-12);
    (u.ln() / 0.2_f64.ln()).floor() as u64
}
/// The walk ends by its own time: `now` reached `until`.
pub fn search_ended(now: u64, until: u64) -> bool {
    now >= until
}
