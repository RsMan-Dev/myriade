//! A level's custom spawners (`ServerLevel.customSpawners`) and their tick
//! (`ServerLevel.tickCustomSpawners`): vanilla's, minus what a mod's
//! `modify_custom_spawners` took out, plus the mod spawners it added.
use super::Game;
use mod_api::{CustomSpawners, SpawnerEntry, SpawnerKind};

impl Game<'_> {
    /// The level's list, built (and `modify_custom_spawners` fired) on first use.
    fn level_spawners(&mut self) -> &CustomSpawners {
        let dim = self.dim;
        if !self.creatures.custom_spawners.contains_key(&dim) {
            let mut list = CustomSpawners::vanilla();
            self.creatures.custom_spawners.insert(dim, list);
        }
        &self.creatures.custom_spawners[&dim]
    }

    /// Whether vanilla's `kind` is still in this level's list.
    pub(super) fn has_custom_spawner(&mut self, kind: SpawnerKind) -> bool {
        self.level_spawners().contains(&SpawnerEntry::Builtin(kind))
    }
}
