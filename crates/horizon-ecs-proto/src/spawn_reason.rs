//! `EntitySpawnReason`: why an entity entered the level. The species' `finalizeSpawn` reads it (a converted zombie draws no pick-up
//! roll and takes no default gear; a jockey, a husk, a drowned, a strider, a piglin, a guardian, a villager, a warden, a trader llama,
//! a zombie horse and an axolotl each branch on it), as do the spawn rules (`isSpawner`, `ignoresLightRequirements`). The host says it
//! with the spawn; the entity keeps it (`Spawn`) for those rules.
use bevy_ecs::prelude::*;

/// The jar's `EntitySpawnReason`, in its order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SpawnReason {
    Natural,
    ChunkGeneration,
    Spawner,
    Structure,
    Breeding,
    MobSummoned,
    Jockey,
    Event,
    Conversion,
    Reinforcement,
    Triggered,
    Bucket,
    SpawnItemUse,
    Command,
    Dispenser,
    Patrol,
    TrialSpawner,
    Load,
    DimensionTravel,
}

impl SpawnReason {
    /// `EntitySpawnReason.isSpawner`: a spawner block or a trial spawner.
    pub fn is_spawner(self) -> bool {
        matches!(self, Self::Spawner | Self::TrialSpawner)
    }
}

/// Why the entity spawned, kept from its spawn (absent on one loaded from saved data).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
#[component(storage = "SparseSet")]
pub struct Spawn(pub SpawnReason);
