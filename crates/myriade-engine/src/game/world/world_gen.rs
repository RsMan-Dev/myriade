//! The world-generation settings a created save gets (`level.dat`), from the options chosen at creation.
use crate::game::world_options::WorldOptions;
use std::path::Path;

impl WorldOptions {
    /// Writes the creation settings. The flat, single-biome and custom-dimension editors are the client's: none is given.
    pub fn write_world_gen(&self, dir: &Path, seed: i64) -> crate::Result<()> {
        myriade_engine::save::write_world_gen_settings_with(dir, seed, self.world_gen(), None, None, None)
    }
}
