//! Creepers (swelling, explosions) and spiders (wall climbing, daylight
//! neutrality, leaping).
use super::{
    Damage, DamageKind, MobEvent, MobWorld, Species, Who,
    goals::{
        Avoid, AvoidEntity, ClimbOnTopOfPowderSnow, Cx, Float, Goal, HurtByTarget, LeapAtTarget,
        LookAtPlayer, MOVE, MeleeAttack, NearestTarget, RandomLookAround, Stroll, goal_clone,
    },
};
use horizon_core::random::RandomSource;


/// `InfestedBlock.isCompatibleHostBlock`: the blocks an infested block can
/// be hidden in, i.e. the host side of vanilla's host-to-infested map. A
/// silverfish prefers these as a walking target.
pub fn is_infested_host(name: &str) -> bool {
    matches!(
        name,
        "minecraft:stone"
            | "minecraft:cobblestone"
            | "minecraft:stone_bricks"
            | "minecraft:mossy_stone_bricks"
            | "minecraft:cracked_stone_bricks"
            | "minecraft:chiseled_stone_bricks"
            | "minecraft:deepslate"
    )
}
/// `Silverfish.SilverfishWakeUpFriendsGoal`: 20 ticks after being hurt, the
/// infested blocks within 10 blocks sideways and 5 up or down break (or turn
/// back into stone with mob griefing off), until a coin flip stops it.
#[derive(Clone, Debug)]
struct SilverfishWakeUpFriends;
impl Goal for SilverfishWakeUpFriends {
    goal_clone!();
    fn name(&self) -> &'static str {
        "SilverfishWakeUpFriendsGoal"
    }
    fn flags(&self) -> u8 {
        0
    }
}
/// `Silverfish.SilverfishMergeWithStoneGoal`: a stroll that, now and then
/// with mob griefing on, hides in a stone block next to the silverfish.
#[derive(Clone, Debug)]
struct SilverfishMergeWithStone {
    stroll: Stroll,
    merge: Option<[i32; 3]>,
}
impl Default for SilverfishMergeWithStone {
    fn default() -> Self {
        Self {
            stroll: Stroll::new(1.0).interval(10),
            merge: None,
        }
    }
}
impl Goal for SilverfishMergeWithStone {
    goal_clone!();
    fn name(&self) -> &'static str {
        "SilverfishMergeWithStoneGoal"
    }
    fn flags(&self) -> u8 {
        MOVE
    }
}
/// Spider.SpiderAttackGoal: gives up in bright light 1 in 100 ticks.
#[derive(Clone, Debug)]
pub struct SpiderAttack(MeleeAttack);
impl Goal for SpiderAttack {
    goal_clone!();
    fn flags(&self) -> u8 {
        self.0.flags()
    }
    fn every_tick(&self) -> bool {
        true
    }
}
/// SwellGoal: fuse within 3 blocks, defuse beyond 7 or out of sight.
#[derive(Clone, Debug, Default)]
pub struct Swell {
    target: Option<Who>,
}
impl Goal for Swell {
    goal_clone!();
    fn flags(&self) -> u8 {
        MOVE
    }
    fn every_tick(&self) -> bool {
        true
    }
}


#[cfg(test)]
mod spider_vehicle_tests {
    use super::*;
    use crate::mobs::world::{test_player, with_test_world};
    use horizon_core::geometry::Vec3;
    use std::sync::Arc;

}
