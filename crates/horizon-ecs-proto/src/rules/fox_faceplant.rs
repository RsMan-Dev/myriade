//! The faceplant's countdown: its selector ticks, two game ticks each.

/// The game ticks of a countdown in selector ticks.
pub fn game_ticks(selector_ticks: i32) -> i32 {
    2 * selector_ticks
}
