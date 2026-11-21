//! The rule of one worn or held piece at death: whether it falls, and with what damage.
use horizon_core::random::RandomSource;

/// `Mob.DEFAULT_EQUIPMENT_DROP_CHANCE`.
pub const DEFAULT: f32 = 0.085;

/// The roll of one piece. `chance` is the stored value, `raised` the one after Looting. Returns the damage the piece falls with, or None
/// when it stays. `own` is the damage it has (kept when preserved); `max` its durability (0: none).
pub fn roll_piece(chance: f32, raised: f32, killed_by_player: bool, prevented: bool, max: i32, own: i32, random: &mut impl RandomSource) -> Option<i32> {
    if chance == 0.0 {
        return None;
    }
    let preserved = chance > 1.0;
    if prevented || !(killed_by_player || preserved) {
        return None;
    }
    if random.next_float() >= raised {
        return None;
    }
    if preserved || max <= 0 {
        return Some(own);
    }
    let inner = random.next_int_bound((max - 3).max(1)).unwrap_or(0);
    Some(max - random.next_int_bound(1 + inner).unwrap_or(0))
}
