//! Striders (`Strider`): their goals, standing and floating on lava
//! (`floatStrider`, `canStandOnFluid`), going cold away from warm blocks
//! (`setSuffocating`) and heading back to lava (`StriderGoToLavaGoal`).
//! `extra.flag` is the synced `DATA_SUFFOCATING`.
use super::{
    MobEvent, MobWorld, Species,
    data::PathComputationType,
    goals::{
        Breed, Cx, FollowParent, Goal, JUMP, LookAtPlayer, MOVE, MoveToBlock, Panic,
        RandomLookAround, Stroll, Tempt, goal_clone, reduced,
    },
    path::PathType,
};
use crate::blocks::Fluid;
use horizon_core::random::RandomSource;

/// `SUFFOCATING_MODIFIER`: `-0.34F`, `ADD_MULTIPLIED_BASE`.
const SUFFOCATING: f64 = -0.34_f32 as f64;

/// `StriderGoToLavaGoal.isValidTarget`: lava with room to stand above.
fn lava_target(w: &MobWorld, p: [i32; 3]) -> bool {
    w.block_name(p[0], p[1], p[2]) == "minecraft:lava"
        && w.data.pathfindable(
            w.world.block(p[0], p[1] + 1, p[2]),
            PathComputationType::Land,
        )
}
/// `Strider.StriderGoToLavaGoal`: a `MoveToBlockGoal` for the nearest lava,
/// aiming at the lava block itself rather than the one above.
#[derive(Clone, Debug)]
struct GoToLava(MoveToBlock);
impl Goal for GoToLava {
    goal_clone!();
    fn flags(&self) -> u8 {
        MOVE | JUMP
    }
    fn name(&self) -> &'static str {
        "StriderGoToLavaGoal"
    }
    fn every_tick(&self) -> bool {
        true
    }
}
