//! The rules of the worn armor: the sum of the pieces over the species' base, floored as `armor::values`.

/// The armor value (floored) and toughness of an entity of base `base` wearing `pieces` (each `(armor, toughness)`), and whether any
/// piece gives something.
pub fn armor_totals(base: (f32, f32), pieces: &[(f64, f64)]) -> (f32, f32, bool) {
    let (mut armor, mut toughness, mut worn) = (f64::from(base.0), f64::from(base.1), false);
    for &(a, t) in pieces {
        worn |= a != 0.0 || t != 0.0;
        armor += a;
        toughness += t;
    }
    (armor.floor() as f32, toughness as f32, worn)
}
