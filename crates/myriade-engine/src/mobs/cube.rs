use horizon_core::{
    geometry::{Vec3, trig},
    random::RandomSource,
};


// AnimState slots.
const SQUISH: usize = 0;
const TARGET_SQUISH: usize = 1;

/// CubeMobFloatGoal
#[derive(Clone, Debug)]
pub(super) struct CubeFloat;
/// CubeMobAttackGoal: face the target and hop at it for up to 300 ticks.
#[derive(Clone, Debug)]
struct CubeAttack {
    tired: i32,
}
/// CubeMobRandomDirectionGoal
#[derive(Clone, Debug, Default)]
pub(super) struct CubeRandomDirection {
    degrees: f32,
    next: i32,
}
/// CubeMobKeepOnJumpingGoal
#[derive(Clone, Debug)]
pub(super) struct CubeKeepJumping;

#[cfg(test)]
mod golem_touch_tests {
    use super::*;
    use crate::mobs::world::with_test_world;
    use std::sync::Arc;

    /// The golems a cube of this kind and size, `x` blocks east of an iron
    /// golem, hurts in one `ai_step`, and the damage.
    fn hits(name: &str, size: i32, x: f64) -> Option<Vec<(u32, f32)>> {
        hits_with(name, size, x, false)
    }

    /// `AbstractCubeMob.push`: a slime or magma cube the iron golem bumps
    /// into hurts it like a touching player (#3513), for `getAttackDamage`;
    /// a tiny slime (`canDealDamage`) and a cube clear of the golem do not.
    #[test]
    #[ignore = "daily"]
    fn a_cube_touching_an_iron_golem_hurts_it() {
        let Some(slime) = hits("minecraft:slime", 2, 1.0) else {
            return;
        };
        assert_eq!(slime, vec![(2, 2.0)], "a size 2 slime hits the golem for 2");
        let magma = hits("minecraft:magma_cube", 4, 1.5).unwrap();
        assert_eq!(magma, vec![(2, 6.0)], "a size 4 magma cube hits for 4 + 2");
        let tiny = hits("minecraft:magma_cube", 1, 0.9).unwrap();
        assert_eq!(
            tiny,
            vec![(2, 3.0)],
            "a tiny magma cube still hits for 1 + 2"
        );
        assert!(
            hits("minecraft:slime", 1, 0.9).unwrap().is_empty(),
            "a tiny slime never hurts"
        );
        assert!(
            hits("minecraft:slime", 2, 3.0).unwrap().is_empty(),
            "the golem is out of touch"
        );
        assert!(
            hits_with("minecraft:slime", 2, 1.0, true)
                .unwrap()
                .is_empty(),
            "a NoAI cube is not isEffectiveAi, so it cannot deal damage"
        );
    }

}

#[cfg(test)]
mod keep_jumping_tests {
    use super::*;
    use crate::mobs::world::with_test_world;
    use std::sync::Arc;


}
