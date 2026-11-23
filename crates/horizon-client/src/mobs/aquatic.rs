//! Water mobs: squid and glow squid (tentacle propulsion), cod, salmon,
//! tropical fish and pufferfish (`AbstractFish`, puffing), and dolphins
//! (breathing, smooth swimming).
use super::{
    Damage, MobEvent, MobWorld, Species, Who,
    anim::{Poses, State},
    control::MoveKind,
    goals::{
        Avoid, AvoidEntity, BreathAir, Cx, Goal, HurtByTarget, LookAtPlayer, MeleeAttack, Panic,
        RandomLookAround, Stroll, TryFindWater, goal_clone, reduced,
    },
    nav::NavKind,
};
use crate::blocks::Fluid;
use crate::health::DamageLevel;
use crate::join_seq::JoinList;
use crate::particles::ParticleType;
use horizon_core::geometry::trig::Mth;
use horizon_core::{geometry::Vec3, random::RandomSource};
use horizon_world::move_gate::MoveGate;
use std::f32::consts::PI;

// Squid animation slots in AnimState.v.
const TENTACLE_MOVEMENT: usize = 0;
pub const TENTACLE_ANGLE: usize = 1;
const X_BODY_ROT: usize = 2;
const Z_BODY_ROT: usize = 3;
const TENTACLE_SPEED: usize = 4;
const ROTATE_SPEED: usize = 5;

/// Pufferfish.getScale by puff state (the dimensions scale with it).
/// `Salmon.Variant.boundingBoxScale`: SMALL 0.5, MEDIUM 1.0, LARGE 1.5.
pub fn salmon_scale(variant: u8) -> f32 {
    match variant {
        0 => 0.5,
        2 => 1.5,
        _ => 1.0,
    }
}
pub fn puff_scale(state: i32) -> f64 {
    match state {
        0 => 0.5,
        1 => 0.7,
        _ => 1.0,
    }
}

/// `EntityTypeTags.NOT_SCARY_FOR_PUFFERFISH`: the mobs a pufferfish leaves
/// alone, its own kind and the water mobs it shares a tank with.
pub fn not_scary_for_pufferfish(s: Species) -> bool {
    matches!(
        s,
        Species::Cod
            | Species::Salmon
            | Species::TropicalFish
            | Species::Pufferfish
            | Species::Squid
            | Species::GlowSquid
            | Species::Dolphin
            | Species::Turtle
            | Species::Guardian
            | Species::ElderGuardian
            | Species::Tadpole
            | Species::Nautilus
            | Species::ZombieNautilus
            | Species::SulfurCube
    )
}











/// Squid.SquidRandomMovementGoal
#[derive(Clone, Debug)]
pub struct SquidRandomMovement;
impl Goal for SquidRandomMovement {
    goal_clone!();
    fn flags(&self) -> u8 {
        0
    }
}
/// Squid.SquidFleeGoal (keeps vanilla's discarded normalize()).
#[derive(Clone, Debug)]
pub struct SquidFlee {
    ticks: i32,
}
impl Goal for SquidFlee {
    goal_clone!();
    fn flags(&self) -> u8 {
        0
    }
    fn every_tick(&self) -> bool {
        true
    }
}
/// `AbstractSchoolingFish`: cod, salmon and tropical fish.
fn schooling(s: Species) -> bool {
    matches!(s, Species::Cod | Species::Salmon | Species::TropicalFish)
}
/// `getMaxSchoolSize`: `AbstractFish.getMaxSpawnClusterSize` (8), a salmon's
/// own 5.
fn max_school_size(s: Species) -> i32 {
    if s == Species::Salmon { 5 } else { 8 }
}
/// `isFollower`: its leader is alive.
fn follows(w: &MobWorld, leader: Option<u32>) -> bool {
    leader.and_then(|l| w.seen(l)).is_some_and(|s| s.alive)
}
/// `canBeFollowed`: it has followers and room for more.
fn can_be_followed(s: Species, followers: i32) -> bool {
    followers > 0 && followers + 1 < max_school_size(s)
}
/// `AbstractFish.FishSwimGoal`: random swimming, which a schooling fish
/// that follows a leader leaves (`canRandomSwim`).
#[derive(Clone, Debug)]
pub struct FishSwim(Stroll);
impl Goal for FishSwim {
    goal_clone!();
    fn name(&self) -> &'static str {
        "FishSwimGoal"
    }
    fn flags(&self) -> u8 {
        self.0.flags()
    }
}
/// `FollowFlockLeaderGoal`: a fish with no school, every ten seconds or so,
/// joins a school nearby that has room, or gathers the fish around it that
/// follow no one into a school of its own; a follower swims after its
/// leader while it is within 11 blocks.
#[derive(Clone, Debug)]
pub struct FollowFlockLeader {
    next_start: i32,
    recalc: i32,
}
impl FollowFlockLeader {
    pub fn new(next_start: i32) -> Self {
        Self {
            next_start,
            recalc: 0,
        }
    }
}
impl Goal for FollowFlockLeader {
    goal_clone!();
    fn set_start_delay(&mut self, ticks: i32) {
        self.next_start = ticks;
    }
    fn name(&self) -> &'static str {
        "FollowFlockLeaderGoal"
    }
    fn flags(&self) -> u8 {
        0
    }
}
/// Pufferfish.PufferfishPuffGoal: inflate when a mob or player is within 2.
#[derive(Clone, Debug)]
pub struct PufferfishPuff;
impl Goal for PufferfishPuff {
    goal_clone!();
    fn flags(&self) -> u8 {
        0
    }
}
/// finalizeSpawn variants (tropical fish patterns and colours).
/// TropicalFish.COMMON_VARIANTS: (pattern index, base colour, pattern colour).
pub const COMMON_TROPICAL: [(u8, u8, u8); 22] = [
    (7, 1, 7),
    (6, 7, 7),
    (6, 7, 11),
    (11, 0, 7),
    (1, 11, 7),
    (0, 1, 0),
    (5, 6, 3),
    (9, 10, 4),
    (11, 0, 14),
    (5, 0, 4),
    (8, 0, 7),
    (11, 0, 1),
    (3, 9, 6),
    (4, 5, 3),
    (10, 14, 0),
    (2, 7, 14),
    (9, 14, 0),
    (6, 0, 4),
    (0, 14, 0),
    (1, 7, 0),
    (3, 9, 4),
    (6, 4, 4),
];
#[cfg(test)]
mod school_tests {
    use super::*;
    use crate::mobs::sensor_scan_tests::kind;
    use std::sync::Arc;

    #[test]
    fn school_sizes() {
        assert_eq!(max_school_size(Species::Cod), 8);
        assert_eq!(max_school_size(Species::TropicalFish), 8);
        assert_eq!(max_school_size(Species::Salmon), 5);
        // canBeFollowed: followers and room (schoolSize below the maximum).
        assert!(!can_be_followed(Species::Cod, 0));
        assert!(can_be_followed(Species::Cod, 6));
        assert!(!can_be_followed(Species::Cod, 7));
        assert!(!can_be_followed(Species::Salmon, 4));
    }
    #[test]
    fn a_recruit_another_leader_took_stays_with_it() {
        let mut mobs: JoinList<_> = [cod(1), cod(2), cod(3)].into_iter().collect();
        start_following(&mut mobs, 3, 1);
        assert_eq!(mobs[2].extra.leader, Some(1));
        assert_eq!(mobs[0].extra.followers, 1);
        start_following(&mut mobs, 3, 2);
        assert_eq!(mobs[2].extra.leader, Some(1));
        assert_eq!(mobs[1].extra.followers, 0);
        // A dead leader's follower is free again.
        mobs[0].health = 0.0;
        start_following(&mut mobs, 3, 2);
        assert_eq!(mobs[2].extra.leader, Some(2));
        assert_eq!(mobs[1].extra.followers, 1);
    }
    #[test]
    fn schooling_fish_draw_their_start_delay() {
        let mut m = cod(1);
        let start = FollowFlockLeader::next_start_tick(&mut m);
        assert!((100..=110).contains(&start));
    }
}

#[cfg(test)]
mod pufferfish_sting_tests {
    use super::*;
    use crate::mobs::apply_brain_events;
    use crate::mobs::world::with_test_world;
    use std::sync::Arc;



    /// `Pufferfish.aiStep`: a puffed pufferfish stings every mob whose box
    /// meets its own (inflated by 0.3), not only the player, for
    /// `getPuffState() + 1`.
    #[test]
    #[ignore = "daily"]
    fn a_puffed_pufferfish_stings_the_mobs_that_touch_it() {
        let mut test_level = crate::health::TestLevel::new(0);
        let level = &mut test_level.level();
        let Some(half) = stings(level, 1, &[("minecraft:zombie", 0.5)]) else {
            return;
        };
        assert_eq!(half, vec![(2, 2.0)], "puff state 1 stings for 1 + 1");
        let full = stings(level, 2, &[("minecraft:zombie", 0.5)]).unwrap();
        assert_eq!(full, vec![(2, 3.0)], "puff state 2 stings for 1 + 2");
        assert!(
            stings(level, 0, &[("minecraft:zombie", 0.5)])
                .unwrap()
                .is_empty(),
            "a deflated pufferfish stings nobody"
        );
        assert!(
            stings(level, 2, &[("minecraft:zombie", 1.5)])
                .unwrap()
                .is_empty(),
            "a zombie a block away never touches it"
        );
        // `#not_scary_for_pufferfish`: its tank mates are left alone.
        assert!(
            stings(
                level,
                2,
                &[("minecraft:cod", 0.4), ("minecraft:squid", 0.3)]
            )
            .unwrap()
            .is_empty(),
            "a cod and a squid are not scary"
        );
    }


}

#[cfg(test)]
mod squid_tests {
    use super::*;
    use std::sync::Arc;


}

