//! `LivingEntity.onClimbable` for mobs: the block a mob stands in, and the mods' say on whether it
//! is a ladder (`native_is_ladder`) and on whether the block under an open trapdoor makes the
//! trapdoor one (`native_makes_open_trapdoor_above_climbable`).
use crate::mobs::{MobWorld};
use horizon_core::position::BlockPos;

