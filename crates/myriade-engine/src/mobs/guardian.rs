//! Guardians and elder guardians (`Guardian`, `ElderGuardian`): the laser
//! charging for 80 (60) ticks before it strikes, spikes that hurt melee
//! attackers while still, flopping on land, the swimming move control, and
//! the elder's mining fatigue curse every minute.
use super::{
    Damage, DamageKind, MobEvent, MobWorld, Species, Who,
    anim::{Poses, State},
    control::{MoveKind, Operation, rotlerp},
    goals::{
        Cx, Goal, LOOK, LookAtPlayer, MOVE, NearestTarget, RandomLookAround, Stroll, goal_clone,
    },
    nav::NavKind,
    path::PathType,
    trader::MoveTowardsRestriction,
};
use crate::health::DamageLevel;
use horizon_core::geometry::trig::Mth;
use horizon_core::{geometry::Vec3, random::RandomSource};
use horizon_world::move_gate::MoveGate;
use std::f32::consts::PI;

// Guardian state: `flag` an active beam target (DATA_ID_ATTACK_TARGET),
// `flag2` moving (DATA_ID_MOVING), `counter2` the client attack time.
// AnimState slots: tail animation, its speed, spikes animation.
pub const TAIL: usize = 0;
const TAIL_SPEED: usize = 1;
pub const SPIKES: usize = 2;
const EYE_X: usize = 3;
const EYE_Y: usize = 4;

/// `Guardian.class` for `LookAtPlayerGoal`: an elder guardian is a guardian.
const GUARDIAN_CLASS: &[Species] = &[Species::Guardian, Species::ElderGuardian];
/// `GuardianAttackGoal`.
#[derive(Clone, Debug)]
struct GuardianAttack {
    attack_time: i32,
}
impl Goal for GuardianAttack {
    goal_clone!();
    fn flags(&self) -> u8 {
        MOVE | LOOK
    }
    fn every_tick(&self) -> bool {
        true
    }
    /// `GuardianAttackGoal.stop`: the guardian strolls as soon as the beam ends.
    fn strolls_on_stop(&self) -> bool {
        true
    }
}
/// The GUARDIAN_ELDER_EFFECT parameter of an elder's curse
/// (`isSilent() ? 0.0F : 1.0F`); the client plays the curse sound only for 1.
pub fn elder_curse_param(silent: bool) -> f32 {
    if silent { 0.0 } else { 1.0 }
}
#[cfg(test)]
mod elder_home_tests {
    use super::*;
    use crate::mobs::world::with_test_world;
    use std::sync::Arc;


}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mobs::world::with_test_world;
    use std::sync::Arc;


    /// `Guardian.aiStep`: a guardian out of water flops, and the flop is heard
    /// (the elder's own sound) unless the guardian is silent (`!isSilent()`).
    #[test]
    #[ignore = "daily"]
    fn a_silent_guardian_flops_without_a_sound() {
        for (kind, sound) in [
            ("minecraft:guardian", "entity.guardian.flop"),
            ("minecraft:elder_guardian", "entity.elder_guardian.flop"),
        ] {
            for silent in [false, true] {
                let Some((mob, events)) = flop(kind, silent) else {
                    return;
                };
                assert!(!mob.on_ground && mob.velocity.y > 0.0, "{kind} flops");
                let sounds: Vec<_> = events
                    .iter()
                    .filter_map(|e| match e {
                        MobEvent::Sound(s, ..) => Some(*s),
                        _ => None,
                    })
                    .collect();
                let heard: &[&str] = if silent { &[] } else { &[sound] };
                assert_eq!(sounds, heard, "{kind} silent={silent}");
            }
        }
    }
}
