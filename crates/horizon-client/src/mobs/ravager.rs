//! Ravagers (`Ravager`): charging beasts that trample leaves, bite for 12,
//! get stunned by a raised shield half the time and answer with a roar
//! that hurts and flings everything within 4 blocks.
use super::{
    Damage, MobEvent, MobWorld, Species, Who,
    anim::{Poses, State},
    goals::{Float, HurtByTarget, LookAtPlayer, MeleeAttack, NearestTarget, Stroll},
    path::PathType,
};
use horizon_core::geometry::trig::Mth;
use horizon_core::random::RandomSource;
use std::f32::consts::PI;

// Ravager state: `counter` attackTick, `counter2` stunnedTick,
// `anim_ticks` roarTick.


#[cfg(test)]
mod tests {
    use super::*;
    use horizon_core::geometry::Vec3;
    use std::sync::Arc;




}

#[cfg(test)]
mod sight_tests {
    use super::*;
    use crate::mobs::world::{test_player, with_built_test_world};
    use horizon_core::geometry::Vec3;
    use std::sync::Arc;


}
