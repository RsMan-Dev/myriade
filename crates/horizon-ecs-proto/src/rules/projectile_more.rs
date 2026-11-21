use horizon_core::random::{LegacyRandom, RandomSource};

/// `ThrownEgg.onHit`: the hatch rolls, 1 in `one_in` for a hatchling, then 1 in `many_in` for `many` of them, else one. `None` when no egg hatches.
pub fn hatch_count(rng: &mut LegacyRandom, one_in: i32, many_in: i32, many: u32) -> Option<u32> {
    if rng.next_int_bound(one_in).unwrap_or(1) != 0 {
        return None;
    }
    Some(if rng.next_int_bound(many_in).unwrap_or(1) == 0 { many } else { 1 })
}

/// `ThrownEnderpearl.onHit`: the roll of an endermite, which comes below `chance`.
pub fn endermite_roll(rng: &mut LegacyRandom, chance: f32) -> bool {
    rng.next_float() < chance
}

/// `ThrownExperienceBottle.onHit`: `base` plus two draws below `bound`, the experience of the bottle.
pub fn bottle_xp(rng: &mut LegacyRandom, base: i32, bound: i32) -> i32 {
    base + rng.next_int_bound(bound).unwrap_or(0) + rng.next_int_bound(bound).unwrap_or(0)
}
