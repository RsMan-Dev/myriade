//! A world's biome regions. The record of the regions mods declare (`biome_regions.dat`) left
//! with the mod API: without mods, no region is declared, no record is written, and every
//! dimension keeps its vanilla source. The functions keep their signatures for the dedicated
//! server's `[biome_regions]` settings, which have nothing to choose.
use super::Result;
use horizon_world::worldgen::biome_source::regions::RegionLayout;
use std::{path::Path, sync::Arc};

/// A world's recorded regions: never read, since no record is ever written.
pub struct BiomeRegions;

impl BiomeRegions {
    /// The region layout the generator uses for `dimension`: none, the source stays vanilla.
    pub fn layout(&self, _dimension: &str) -> Option<Arc<RegionLayout>> {
        None
    }
}

/// Nothing is recorded: no mod declares a region.
pub fn create(_world: &Path) -> Result<()> {
    Ok(())
}

/// No record: every dimension keeps its source.
pub fn open(_world: &Path) -> Result<Option<BiomeRegions>> {
    Ok(None)
}

/// No record, so no region is in place for `dimension`.
pub fn layout(_world: &Path, _dimension: &str) -> Result<Option<Arc<RegionLayout>>> {
    Ok(None)
}

/// No mod's region is missing: none is declared.
pub fn unchosen_missing(_world: &Path) -> Vec<(String, String)> {
    Vec::new()
}

/// Nothing to adopt: no mod declares a region.
pub fn adoptable(_world: &Path) -> bool {
    false
}

/// Nothing to choose: no mod declares a region.
pub fn choose(_world: &Path, _vanilla_for_missing: bool, _adopt_installed: bool) -> Result<()> {
    Ok(())
}
