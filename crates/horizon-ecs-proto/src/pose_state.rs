//! The pose an event imposes on a creature (`LivingEntity.setPose`: sleeping, swimming, crouching...), held as its `Pose.id`. Sparse: a
//! creature standing (id 0) holds none.
use bevy_ecs::prelude::*;

#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct PoseOverride(pub u8);

/// Sets the pose id; `None` or standing (0) removes the component.
pub fn set(w: &mut World, e: Entity, pose: Option<u8>) {
    match pose {
        Some(p) if p != 0 => w.entity_mut(e).insert(PoseOverride(p)),
        _ => w.entity_mut(e).remove::<PoseOverride>(),
    };
}

pub fn get(w: &World, e: Entity) -> Option<u8> {
    w.get::<PoseOverride>(e).map(|p| p.0)
}
