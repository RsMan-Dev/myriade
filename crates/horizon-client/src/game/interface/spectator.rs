//! The spectator menu (`SpectatorGui`): its players and teams from the
//! server's player list and teams, and the teleport request.
use super::Game;
use horizon_client::{
    hud::HudBatch,
    hud_layout, keybinds,
    scoreboard::{comp_text, team_color_rgb},
    spectator_menu::{Member, Outcome, SpectatorMenu, Team},
    tab_list::{SPECTATOR, TabPlayer},
    text::Text,
};
use horizon_protocol::packets::play::{self, Serverbound as S};

impl Game<'_> {
    /// `Util.getMillis` for the menu's fade.
    fn spectator_now(&self) -> i64 {
        self.started.elapsed().as_millis() as i64
    }
    fn spectator_outcome(&mut self, outcome: Outcome) {
        if let Outcome::Teleport(uuid) = outcome {
            // PlayerMenuItem.selectItem.
            self.send(S::TeleportToEntity(play::TeleportToEntity { uuid }));
        }
    }
    /// `Hud.extractHotbar` for a spectator, and `extractAction`'s prompt.
    pub(super) fn draw_spectator_hotbar(&mut self, batch: &mut HudBatch) {
        let (w, h) = (batch.width as i32, batch.height as i32);
        let now = self.spectator_now();
        let labels: Vec<Text> = (1..=9)
            .map(|n| keybinds::display_name(self.settings.keys.get(&format!("key.hotbar.{n}"))))
            .collect();
        let font = &self.gui.atlas.font;
        let mut draws = self
            .spectator_gui
            .hotbar(w, h, now, &|slot| labels[slot].clone(), font);
        draws.extend(self.spectator_gui.action(
            w,
            h,
            now,
            font,
            self.settings.text_background(0.0),
        ));
        hud_layout::paint(batch, &self.gui.atlas, &draws);
    }
}
