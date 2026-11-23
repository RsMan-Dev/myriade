//! The wither (`WitherBoss`): eleven seconds of invulnerable charging that
//! end in a blast, then flying after its target while three heads fire
//! skulls, breaking blocks around it when hurt, immune to arrows and a
//! player's wind charges below half health.
use super::{
    MobEvent, MobWorld, Species, Who,
    anim::{Poses, State},
    control::MoveKind,
    goals::{
        Cx, Goal, HurtByTarget, JUMP, LOOK, LookAtPlayer, MOVE, NearestTarget, Prey,
        RandomLookAround, Stroll, goal_clone,
    },
    nav::NavKind,
};
use crate::projectiles::{Projectile, ProjectileKind};
use horizon_core::geometry::trig::Mth;
use horizon_core::{geometry::Vec3, random::RandomSource};

// Wither state: `counter` the invulnerable ticks, `counter2` the block
// breaking clock, `point` the main target's position. `ints`: the side
// heads' targets (mob id, -1 the player, 0 none), their next update ticks
// and idle counts. AnimState slots 0-1 the side heads' yaw, 2-3 their pitch.
pub const INVULNERABLE: i32 = 220;

/// `WitherBoss.hurtServer`: a wither takes nothing from a `#wither_friends`
/// mob (the undead, other withers included) that is not a player.
pub fn shielded_from(victim: Species, attacker: Species) -> bool {
    victim == Species::Wither && undead(attacker)
}
/// `#wither_friends`: the undead.
fn undead(s: Species) -> bool {
    matches!(
        s,
        Species::Zombie
            | Species::Husk
            | Species::Drowned
            | Species::ZombieVillager
            | Species::Skeleton
            | Species::Stray
            | Species::Bogged
            | Species::Parched
            | Species::WitherSkeleton
            | Species::ZombieHorse
            | Species::SkeletonHorse
            | Species::Phantom
            | Species::ZombifiedPiglin
            | Species::Zoglin
            | Species::Wither
    )
}
/// `WitherDoNothingGoal`: still while charging.
#[derive(Clone, Debug)]
struct DoNothing;
impl Goal for DoNothing {
    goal_clone!();
    fn flags(&self) -> u8 {
        MOVE | JUMP | LOOK
    }
}
fn target_pos(w: &MobWorld, id: i32) -> Option<([f64; 3], f64, bool)> {
    match id {
        -1 => w
            .selected_player()
            .filter(|p| p.alive && !p.creative && !p.spectator)
            .map(|p| ([p.position.x, p.position.y, p.position.z], p.eye[1], true)),
        0 => None,
        id => w
            .others
            .iter()
            .find(|s| s.id == id as u32 && s.alive)
            .map(|s| ([s.position.x, s.position.y, s.position.z], s.eye_y, false)),
    }
}
/// `WitherBoss.rotlerp`: turn at most `max` degrees toward `b`, the result
/// not wrapped (unlike `MoveControl.rotlerp`, which keeps it in 0..360).
fn head_rotlerp(a: f32, b: f32, max: f32) -> f32 {
    a + super::living::wrap(b - a).clamp(-max, max)
}
#[cfg(test)]
mod tests {
    use super::*;
}

#[cfg(test)]
mod upstream_sync_tests {
    use super::*;
    #[test]
    fn head_rotlerp_keeps_negative_angles_unwrapped() {
        assert_eq!(head_rotlerp(-5.0, -20.0, 40.0), -20.0);
        assert_eq!(head_rotlerp(-5.0, -100.0, 40.0), -45.0);
        assert_eq!(head_rotlerp(355.0, 10.0, 10.0), 365.0);
        assert_eq!(head_rotlerp(2.0, -30.0, 10.0), -8.0);
        assert_eq!(head_rotlerp(-170.0, 170.0, 10.0), -180.0);
    }
    #[test]
    fn head_rotlerp_steps_are_bounded() {
        let (mut yaw, mut pitch) = (30.0_f32, 0.0_f32);
        for _ in 0..100 {
            let (ny, np) = (
                head_rotlerp(yaw, -100.0, 10.0),
                head_rotlerp(pitch, -80.0, 40.0),
            );
            assert!((ny - yaw).abs() <= 10.0 && (np - pitch).abs() <= 40.0);
            (yaw, pitch) = (ny, np);
        }
        assert_eq!((yaw, pitch), (-100.0, -80.0));
    }
    #[test]
    fn wither_ignores_wither_friends_only() {
        assert!(shielded_from(Species::Wither, Species::Wither));
        assert!(shielded_from(Species::Wither, Species::Skeleton));
        assert!(!shielded_from(Species::Wither, Species::Creeper));
        assert!(!shielded_from(Species::Zombie, Species::Wither));
    }
}
