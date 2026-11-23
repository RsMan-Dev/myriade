//! Structure mobs: `Mob.finalizeSpawn(level, difficulty, STRUCTURE, null)` for the
//! mobs structures place. The mob engine left with the mod API, so nothing is
//! finalized here: a generated mob keeps the entity it was placed with.
use horizon_core::nbt::Compound;
use horizon_world::{
    item_defaults::ItemData,
    worldgen::{generator::StructureMobs, structure::legacy::PieceRandom},
};
use std::{path::PathBuf, sync::Arc};

pub struct StructureMobFinalizer;

impl StructureMobFinalizer {
    pub fn new(_root: PathBuf, _dimension: &str, _item_data: Arc<ItemData>) -> Self {
        Self
    }
    pub fn set_mob_maps(&self, _snapshot: &Arc<crate::neoforge_mob_maps::NeoForgeMobMaps>) {}
    pub fn set_conditions(&self, _difficulty: u8, _day_time: i64) {}
    pub fn load_behind(&self) {}
}

impl StructureMobs for StructureMobFinalizer {
    fn finalize(
        &self,
        _nbt: &mut Compound,
        _biome: &str,
        _structures: &[String],
        _random: &mut dyn PieceRandom,
    ) -> bool {
        true
    }
}
