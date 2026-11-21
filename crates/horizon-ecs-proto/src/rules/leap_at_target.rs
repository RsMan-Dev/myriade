//! The wait of the next leap ask (`leap_at_target`): the asks fall on the phase ticks, after a count of failed draws.

/// The ticks until the next ask: one or two to the phase tick, then two per failure.
pub fn ask_wait(now: u64, phase: u64, failures: u64) -> u64 {
    let first = if (now + 1 + phase) % 2 == 0 { 1 } else { 2 };
    first + 2 * failures
}
