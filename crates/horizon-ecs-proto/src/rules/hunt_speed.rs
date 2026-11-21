//! Speed of a ravager (`Ravager.aiStep`, `ravager.rs:98-111`): the base moves 10 % of the way to its mode each tick.

/// The speed mode: the hunting speed while it has a target, else the idle speed.
pub fn pace_max(hunting: bool, hunting_speed: f64, idle_speed: f64) -> f64 {
    if hunting { hunting_speed } else { idle_speed }
}

/// The speed after `elapsed` ticks from `base`, approaching `max`; 0 while busy.
pub fn paced_speed(base: f64, max: f64, busy: bool, elapsed: u64) -> f64 {
    if busy {
        0.0
    } else if elapsed == 0 {
        base
    } else {
        max - (max - base) * 0.9_f64.powi(elapsed as i32)
    }
}
