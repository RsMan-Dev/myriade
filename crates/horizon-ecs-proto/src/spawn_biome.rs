//! The biome of a position for the spawn pass: the biome `BiomeManager.getBiome` reads there (the zoom of the level's seed, `mobs::zoomed_biome_index`).
//! Its index is into `mw.biomes`, the index `Spawner.tables` is built by.
use myriade_engine_lib::mobs::spawn_rules::SpawnLevel;
use myriade_engine_lib::mobs::{zoomed_biome_index, MobWorld};
use horizon_core::biome::{obfuscate_seed, BiomeZoom};

/// The zoom of the world's biomes: its seed obfuscated, the one hash of a world (kept by `Spawner`, made by `zoom_of` once).
pub fn zoom_of(mw: &MobWorld) -> BiomeZoom {
    BiomeZoom::new(obfuscate_seed(mw.world_seed()))
}
/// The index in `mw.biomes` of the biome at a block; a cell not loaded reads the first biome.
pub fn biome_at(mw: &MobWorld, zoom: BiomeZoom, x: i32, y: i32, z: i32) -> u16 {
    zoomed_biome_index(mw, zoom, x, y, z).unwrap_or(0)
}
