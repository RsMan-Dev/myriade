//! The data of a trial spawner's configs (`TrialSpawnerConfig`) and of the mob a spawner spawns (`SpawnData`): plain data, read and
//! written by the host (`myriade-engine`'s `trial_config`).
use horizon_core::nbt::{Compound, Tag};

/// `SpawnData.CustomSpawnRules`: the block and sky light the spawn block
/// must have (inclusive ranges within 0..=15).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CustomSpawnRules {
    pub block: (i32, i32),
    pub sky: (i32, i32),
}
/// `SpawnData`: the entity to spawn, optional light rules and an equipment
/// table (kept as loaded).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SpawnData {
    pub entity: Compound,
    pub custom_spawn_rules: Option<CustomSpawnRules>,
    pub equipment: Option<Tag>,
}
/// `TrialSpawnerConfig`.
#[derive(Clone, Debug, PartialEq)]
pub struct TrialConfig {
    pub spawn_range: i32,
    pub total_mobs: f32,
    pub simultaneous_mobs: f32,
    pub total_mobs_added_per_player: f32,
    pub simultaneous_mobs_added_per_player: f32,
    pub ticks_between_spawn: i32,
    pub spawn_potentials: Vec<(SpawnData, i32)>,
    pub loot_tables_to_eject: Vec<(String, i32)>,
    pub items_to_drop_when_ominous: String,
}
impl Default for TrialConfig {
    /// `TrialSpawnerConfig.Builder`'s defaults.
    fn default() -> Self {
        Self {
            spawn_range: 4,
            total_mobs: 6.0,
            simultaneous_mobs: 2.0,
            total_mobs_added_per_player: 2.0,
            simultaneous_mobs_added_per_player: 1.0,
            ticks_between_spawn: 40,
            spawn_potentials: Vec::new(),
            loot_tables_to_eject: vec![
                ("minecraft:spawners/trial_chamber/consumables".into(), 1),
                ("minecraft:spawners/trial_chamber/key".into(), 1),
            ],
            items_to_drop_when_ominous:
                "minecraft:spawners/trial_chamber/items_to_drop_when_ominous".into(),
        }
    }
}
