//! `MeleeAttack` (`brain/behaviours/melee_attack.rs`): the melee reach and the blow's damage.

/// `Mob.isWithinMeleeAttackRange`'s inflation: `sqrt(2.04) - 0.6`, on x and z.
pub fn reach() -> f64 {
    2.04_f32.sqrt() as f64 - 0.6
}
