//! Calculations of the cat's sitting on a block: the countdown drawn at a search, and the ticks until the next one.
use crate::rules::cat_state::reduced;

/// The countdown in asks: `reduced(delay + draw)`, `draw` being the `nextInt(delay)` of the search.
pub fn countdown(delay: i32, draw: i32) -> i32 {
    reduced(delay + draw)
}

/// The ticks until the next search: the asks of the countdown, two ticks each, plus one.
pub fn search_ticks(countdown: i32) -> i32 {
    2 * (countdown + 1)
}
