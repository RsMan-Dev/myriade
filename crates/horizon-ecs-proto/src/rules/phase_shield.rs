//! Calculations of the phase shield (`PhaseShield`, invulnerability refusal): when a hit is refused, and when the holder is
//! powered (at or below half its maximum health).

/// Powered: health at or below half the maximum.
pub fn powered_at(health: f32, max_health: f32) -> bool {
    health <= max_health / 2.0
}
/// A hit is refused while charging (unless it bypasses invulnerability), when powered by a direct arrow, or when it comes
/// from an undead attacker or from the holder itself.
pub fn refuses_hit(charging: bool, bypass_invulnerable: bool, powered: bool, direct_arrow: bool, undead_or_self: bool) -> bool {
    (charging && !bypass_invulnerable) || (powered && direct_arrow) || undead_or_self
}
