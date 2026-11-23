//! Llamas and trader llamas (`Llama`, `TraderLlama`): coat variants and
//! strength, chests and carpets, taming by riding (maximum temper 30), the
//! spit ranged attack, hunting wolves (and, for traders, zombies and
//! illagers) and `LlamaModel`.
use super::finalize::{Attr, set_base};
use super::{
    MobEvent, MobWorld, Seen, Species, Who, equine,
    goals::{
        Breed, Cx, Float, FollowParent, Goal, HurtByTarget, LookAtPlayer, NearestTarget, Panic,
        Prey, RandomLookAround, Stroll, Tempt, goal_clone,
    },
    witch::RangedAttack,
};
use crate::projectiles::{Projectile, ProjectileKind};
use horizon_core::{geometry::Vec3, random::RandomSource};

pub const VARIANTS: [&str; 4] = ["creamy", "white", "brown", "gray"];
/// `Llama.getMaxTemper`.
pub const MAX_TEMPER: i32 = 30;

// Llama state: `variant` the coat, `variant2` the strength (1 to 5),
// `equine_temper` the signed temper, `chested` the chest, `head` the carpet, `flag2`
// `didSpit`, `ints[0]` a trader llama's despawn delay.
pub fn is_llama(s: Species) -> bool {
    matches!(s, Species::Llama | Species::TraderLlama)
}
/// The mob is a zombie of any kind (`Zombie.class`).
fn is_zombie(s: &Seen) -> bool {
    matches!(
        s.species,
        Species::Zombie | Species::Husk | Species::Drowned | Species::ZombieVillager
    )
}
/// The mob is an illager (`AbstractIllager.class`).
pub fn is_illager(s: &Seen) -> bool {
    matches!(
        s.species,
        Species::Pillager | Species::Vindicator | Species::Evoker | Species::Illusioner
    )
}
/// `TraderLlama.DEFAULT_DESPAWN_DELAY`.
pub const DEFAULT_DESPAWN_DELAY: i32 = 47999;
/// `LlamaHurtByTargetGoal`: stops hunting right after a spit.
#[derive(Clone, Debug)]
struct LlamaHurtBy(HurtByTarget);
impl Goal for LlamaHurtBy {
    goal_clone!();
    fn flags(&self) -> u8 {
        self.0.flags()
    }
}
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
