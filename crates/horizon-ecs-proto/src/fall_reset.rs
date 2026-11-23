//! `LivingEntity.aiStep`: a creature with levitation or slow falling discards its fall distance before its travel, NoAI or not.
use crate::cow::Contact;
use crate::effects::{Effects, Kind};
use bevy_ecs::prelude::*;

/// Zeroes the fall distance of the creatures with a flight effect; a creature with no effect list is not visited.
pub fn fall_reset(mut q: Query<(&mut Contact, &Effects)>) {
    for (mut c, f) in &mut q {
        if c.fall_distance != 0.0 && f.list.iter().any(|x| matches!(x.kind, Kind::Levitation | Kind::SlowFalling)) {
            c.fall_distance = 0.0;
        }
    }
}
