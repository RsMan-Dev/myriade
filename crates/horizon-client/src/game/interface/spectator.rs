//! The spectator menu's teleport request (`SpectatorGui`).
use super::Game;
use horizon_client::spectator_menu::Outcome;
use horizon_protocol::packets::play::{self, Serverbound as S};

impl Game<'_> {
    fn spectator_outcome(&mut self, outcome: Outcome) {
        if let Outcome::Teleport(uuid) = outcome {
            // PlayerMenuItem.selectItem.
            self.send(S::TeleportToEntity(play::TeleportToEntity { uuid }));
        }
    }
}
