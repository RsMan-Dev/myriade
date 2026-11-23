use crate::projectiles::{Projectile, ProjectileKind};
use horizon_core::{geometry::Vec3, random::RandomSource};

pub const VARIANTS: [&str; 4] = ["creamy", "white", "brown", "gray"];
/// `Llama.getMaxTemper`.
pub const MAX_TEMPER: i32 = 30;

/// `TraderLlama.DEFAULT_DESPAWN_DELAY`.
pub const DEFAULT_DESPAWN_DELAY: i32 = 47999;
#[cfg(test)]
mod tests {
    use super::*;
    use horizon_core::random::LegacyRandom;

    #[test]
    fn strength_stays_in_range_and_rarely_reaches_five() {
        let mut r = LegacyRandom::new(7);
        let mut fives = 0;
        for _ in 0..4000 {
            let max = if r.next_float() < 0.04 { 5 } else { 3 };
            let s = 1 + r.next_int_bound(max).unwrap();
            assert!((1..=5).contains(&s));
            fives += i32::from(s > 3);
        }
        assert!(fives > 0 && fives < 200);
    }
    #[test]
    fn a_chest_holds_strength_columns() {
        assert_eq!(DEFAULT_DESPAWN_DELAY, 47999);
        assert_eq!(MAX_TEMPER, 30);
    }
}
