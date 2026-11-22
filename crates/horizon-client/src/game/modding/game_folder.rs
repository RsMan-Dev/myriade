use crate::game::Game;
use std::path::PathBuf;

impl Game<'_> {
    /// The game folder (the working directory): what its players share, saves, packs, mods.
    pub(in crate::game) fn game_dir() -> PathBuf {
        std::env::current_dir().unwrap_or_default()
    }
}
