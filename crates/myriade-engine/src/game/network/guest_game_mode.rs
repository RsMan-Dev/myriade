//! A guest's `/gamemode` packet, run by the server (`GameModeCommand.setGameMode`).
use super::Game;
use myriade_engine::command::args::GameMode;
use myriade_engine::text::Text;

impl Game<'_> {
    /// Sets the acting player's game mode and tells them of it, when it changed.
    pub(super) fn change_game_mode(&mut self, mode: GameMode) {
        if self.set_game_mode(mode) {
            let name = Text::translate(&format!("gameMode.{}", mode.name()), &[]);
            self.feedback(Text::translate("commands.gamemode.success.self", &[name]));
        }
    }
}
