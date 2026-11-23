//! `Entity.invulnerable` (the `Invulnerable` save key, `/data` and `/summon`): a marker, absent when the entity is not invulnerable.
use bevy_ecs::prelude::*;

/// The entity refuses the damage `isInvulnerableToBase` refuses, for good. Written and read by the `Invulnerable` field of `nbt_fields`.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Invulnerable;
