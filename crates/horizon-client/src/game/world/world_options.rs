//! The options a new world is created with (`WorldOptions`): the difficulty, the commands and the
//! world preset, the game rules and the data packs, as the world's save and its generation take them.
use crate::save::WorldPreset;
use horizon_client::gamerules::GameRules;
use std::{path::PathBuf, sync::Arc};

/// Difficulty ids, in the order of the game's difficulty levels.
pub const DIFFICULTIES: [&str; 4] = ["peaceful", "easy", "normal", "hard"];

/// WorldCreationUiState beyond the name and seed, kept to what the world takes.
#[derive(Clone, Debug, PartialEq)]
pub struct WorldOptions {
    /// Survival locked to Hard, one life (`SelectedGameMode.HARDCORE`).
    pub hardcore: bool,
    /// Index in `DIFFICULTIES`.
    pub difficulty: usize,
    /// Allow Cheats as toggled; unset follows the game mode.
    pub commands: Option<bool>,
    /// The world preset the world is created with.
    pub world_type: WorldPreset,
    pub structures: bool,
    pub bonus_chest: bool,
    /// `getGameRules`: the values saved with the world.
    pub rules: GameRules,
    /// The data packs selected, lowest priority first.
    pub data_packs: Vec<String>,
    /// The folder of packs for the new world.
    pub pack_dir: Option<PathBuf>,
    /// Additional repository sources retained for this creation configuration.
    pub additional_data_packs: Arc<[Arc<crate::datapack::DataPack>]>,
}

impl Default for WorldOptions {
    fn default() -> Self {
        Self {
            hardcore: false,
            difficulty: 2,
            commands: None,
            world_type: WorldPreset::Normal,
            structures: true,
            bonus_chest: false,
            rules: GameRules::default(),
            data_packs: vec!["vanilla".into()],
            pack_dir: None,
            additional_data_packs: Arc::from([]),
        }
    }
}

impl WorldOptions {
    /// `getDifficulty`: Hardcore is always Hard.
    pub fn difficulty(&self) -> usize {
        if self.hardcore { 3 } else { self.difficulty }
    }

    /// `isAllowCommands`: never in Hardcore, else on for Creative until toggled.
    pub fn commands(&self, creative: bool) -> bool {
        !self.hardcore && self.commands.unwrap_or(creative)
    }

    /// `isBonusChest`: never in Hardcore.
    pub fn bonus_chest(&self) -> bool {
        !self.hardcore && self.bonus_chest
    }

    pub fn world_gen(&self) -> crate::save::WorldGen {
        crate::save::WorldGen {
            preset: self.world_type,
            structures: self.structures,
            bonus_chest: self.bonus_chest(),
        }
    }

    /// The packs a world made with these options can take: the feature packs of the resources,
    /// the additional sources and the folder's packs.
    pub fn available_packs(&self, resources: &std::path::Path) -> Vec<crate::datapack::DataPack> {
        let mut packs = crate::datapack::feature_packs(resources);
        packs.extend(self.additional_data_packs.iter().map(|pack| (**pack).clone()));
        if let Some(dir) = &self.pack_dir {
            packs.extend(crate::datapack::discover_folder(dir));
        }
        packs
    }

    /// WorldDataConfiguration's selected feature packs over vanilla's flags.
    pub fn enabled_features(&self, resources: &std::path::Path) -> Vec<String> {
        let mut features = vec!["minecraft:vanilla".to_owned()];
        features.extend(
            self.available_packs(resources)
                .iter()
                .filter(|pack| self.data_packs.contains(&pack.id))
                .flat_map(|pack| pack.features()),
        );
        features.sort();
        features.dedup();
        features
    }
}
