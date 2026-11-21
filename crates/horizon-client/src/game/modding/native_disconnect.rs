//! A client mod leaving the world, NeoForge's `IClientCommonPacketListenerExtension.disconnect`.
//! The server half is `native_player_disconnect`.
use super::Game;
use horizon_client::{Result, text::Text};
use mod_api::WorldEpoch;

/// A client mod's request to leave, carried out at the end of the frame.
pub(super) struct LeaveRequest {
    /// The world it was made in: a request made while that world closes is dropped.
    epoch: WorldEpoch,
    reason: Option<Text>,
}

impl Game<'_> {
    /// The end of the frame: a mod's `disconnect` leaves now, outside every hook. With a
    /// reason, the disconnect screen shows it; with none, the pause menu's Disconnect runs. A
    /// request from a world that has closed since is dropped.
    pub(in crate::game) fn native_leave_if_asked(&mut self) -> Result<()> {
        // Only a client leaves its world: a server drops the request.
        self.native_net.leave = None;
        Ok(())
    }
}
