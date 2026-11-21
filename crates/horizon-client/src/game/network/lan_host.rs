//! Starts the network side of a LAN host or a dedicated server: the hub and its play state.
use super::Game;
use super::lan::LanSettings;
use horizon_client::Result;
use horizon_client::lan::{NetConfig, hub::HubConfig};

impl Game<'_> {
    /// Starts the network side and the play-state hub, for the LAN or for a dedicated server (no host).
    pub(super) fn lan_host(
        &mut self,
        settings: LanSettings,
        net: NetConfig,
        config: HubConfig,
    ) -> Result<()> {
        self.start_lan_hub(settings, net, config)
    }
}
