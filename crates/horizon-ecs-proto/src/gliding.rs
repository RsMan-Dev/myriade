//! `LivingEntity.setFallFlying` (key `FallFlying`): the creature glides with its elytra. The component stores the world's tick of the
//! start; `fall_fly_ticks` is the time since, read on demand (no count per tick).
use crate::cow::now_of;
use bevy_ecs::prelude::*;

#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct FallFlying {
    pub since: u32,
}

/// Starts (`on`) or stops the glide, from the entity's own time now.
pub fn set(w: &mut World, e: Entity, on: bool) {
    if on {
        let since = now_of(w, e) as u32;
        w.entity_mut(e).insert(FallFlying { since });
    } else {
        w.entity_mut(e).remove::<FallFlying>();
    }
}

/// The ticks of the glide so far; 0 when not gliding.
pub fn ticks(w: &World, e: Entity) -> i32 {
    w.get::<FallFlying>(e).map_or(0, |f| (now_of(w, e) as u32).wrapping_sub(f.since) as i32)
}
