//! The rules of the perch: when a perched dragon takes off, and the ticks at which its phases turn (flames, scan, attack, charge).
use crate::rules::dragon_data::{PERCH_LIMIT, SITTING_SCANNING, TAKEOFF};

/// Whether the damage taken while perched, `total` with the new blow, takes the dragon off (`data::PERCH_LIMIT`).
pub fn takes_off(total: f32) -> bool {
    total > PERCH_LIMIT
}

/// Whether the flame is set off on tick `t` of the flaming phase.
pub fn flame_fires(t: i32) -> bool {
    t == 10
}

/// Whether the flaming phase ends on tick `t`.
pub fn flame_ends(t: i32) -> bool {
    t >= 200
}

/// The phase after the flames: the takeoff after four flames, else back to the scan.
pub fn flame_next(flames: i32) -> u8 {
    if flames >= 4 { TAKEOFF } else { SITTING_SCANNING }
}

/// Whether a scan that sees a player starts its attack on tick `t`.
pub fn attack_starts(t: i32) -> bool {
    t > 25
}

/// Whether a scan without a player in reach charges on tick `t`.
pub fn charge_starts(t: i32) -> bool {
    t >= 100
}

/// Whether the attack ends on tick `t`, for the flames.
pub fn attack_ends(t: i32) -> bool {
    t > 40
}
