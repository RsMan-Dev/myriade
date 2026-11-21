//! Calculations of the posture (`Posture`, sitting and standing up): the transition's length, the refusal to move during it,
//! the end of a transition, and the time a standing panda or animal is taken to have stood since.

/// The length of the transition: the sit-down time when sitting, else the stand-up time.
pub fn transition_ticks(sitting: bool, sit_down: u64, stand_up: u64) -> i64 {
    if sitting { sit_down as i64 } else { stand_up as i64 }
}
/// Refuses to move: sitting, or still in the transition of standing up or sitting down.
pub fn refuses(sitting: bool, elapsed: i64, transition: i64) -> bool {
    sitting || elapsed < transition
}
/// Settled: sitting, and the transition is over.
pub fn sat(sitting: bool, elapsed: i64, transition: i64) -> bool {
    sitting && elapsed >= transition
}
/// The start of the posture that counts as already stood up: `stand_up` ticks and one before `now`.
pub fn since_standing(now: i64, stand_up: u64) -> i64 {
    now - stand_up as i64 - 1
}
