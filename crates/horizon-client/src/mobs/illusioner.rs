//! The illusioner (`Illusioner`): a bow-wielding spellcaster that turns
//! invisible behind four mirror images and blinds its target.
use super::{
    MobEvent, Species, Who,
    goals::{
        Avoid, AvoidEntity, Cx, Float, Goal, HurtByTarget, LookAtPlayer, NearestTarget, Stroll,
        goal_clone, reduced,
    },
    illager::CastingSpell,
    undead::BowAttack,
};
use horizon_core::random::RandomSource;

const INVISIBILITY: &str = "minecraft:invisibility";
const BLINDNESS: &str = "minecraft:blindness";
/// `Illusioner.NUM_ILLUSIONS`.
pub const COPIES: usize = 4;

/// `Illusioner.clientSideIllusionTicks` and the two offset sets.
#[derive(Clone, Debug, Default)]
pub struct Illusions {
    pub ticks: i32,
    pub offsets: [[[f64; 3]; COPIES]; 2],
}

// `IllagerSpell` ids: DISAPPEAR (the mirror spell) 4, BLINDNESS 5.
pub const MIRROR: u8 = 4;
pub const BLINDNESS_SPELL: u8 = 5;



/// `IllusionerMirrorSpellGoal` and `IllusionerBlindnessSpellGoal`.
#[derive(Clone, Debug)]
struct Spell {
    spell: u8,
    warmup: i32,
    next: i64,
    last_target: Option<Who>,
}
impl Spell {
    fn new(spell: u8) -> Self {
        Self {
            spell,
            warmup: 0,
            next: 0,
            last_target: None,
        }
    }
    /// (casting time, interval, prepare sound).
    fn timing(&self) -> (i32, i64, &'static str) {
        if self.spell == MIRROR {
            (20, 340, "entity.illusioner.prepare_mirror")
        } else {
            (20, 180, "entity.illusioner.prepare_blindness")
        }
    }
}
impl Goal for Spell {
    goal_clone!();
    fn flags(&self) -> u8 {
        0
    }
}






/// The wobble `IllusionerRenderer.submit` adds to each image.
pub fn wobble(i: usize, age: f32) -> [f64; 3] {
    let i = i as f32;
    [
        f64::from((i + age * 0.5).cos()) * 0.025,
        f64::from((i + age * 0.75).cos()) * 0.0125,
        f64::from((i + age * 0.7).cos()) * 0.025,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn settled_offsets_are_the_new_set() {
        let il = Illusions {
            ticks: 0,
            offsets: [[[1.0; 3]; COPIES], [[2.0; 3]; COPIES]],
        };
        let mut v = [[0.0; 3]; COPIES];
        v.copy_from_slice(&il.offsets[1]);
        assert_eq!(v[0], [2.0; 3]);
    }
    #[test]
    fn wobble_is_small() {
        assert!(wobble(1, 10.0).iter().all(|c| c.abs() <= 0.025));
    }


}
