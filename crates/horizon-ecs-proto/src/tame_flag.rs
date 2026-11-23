//! `TamableAnimal.isTame`, the `DATA_FLAGS_ID` bit 0x04: a marker `tame` inserts. No key of its own in the save: the load sets it
//! from the owner (`TamableAnimal.readAdditionalSaveData` calls `setTame(true)` where an owner is read, `setTame(false)` where none).
use bevy_ecs::prelude::*;

/// The entity is tame. Absent: not tame.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct TameFlag;
