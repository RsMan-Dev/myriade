//! The PvP options the server resolves: the Legacy rules (`legacy_rules.rs`, `docs/pvp.md`).
//! The HUD and look options are drawn by the client and are not kept here.
use crate::game::Game;

#[derive(Default)]
pub(in crate::game) struct PvpState;

impl Game<'_> {
    /// Legacy rules: the hits a server resolves follow 1.8.9 (`legacy_rules.rs`).
    pub(in crate::game) fn pvp_legacy_rules(&self) -> bool {
        self.settings.legacy_combat
    }
    /// The server's legacy rules, from server.toml or the window's options.
    pub(in crate::game) fn set_pvp_legacy_rules(&mut self, on: bool) {
        self.settings.legacy_combat = on;
    }
}
