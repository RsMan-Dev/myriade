//! `Entity.isCurrentlyGlowing` (the `Glowing` save key, `setGlowingTag`): a marker, absent when the entity does not glow.
use bevy_ecs::prelude::*;

/// The entity glows (its flag 0x40 is set). Written and read by the `Glowing` field of `nbt_fields`.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Glowing;
