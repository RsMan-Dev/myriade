//! Host method declarations; access defaults and game forwarders are generated.
#![allow(unused_variables)]
use crate::*;
/// The server host's methods of the `network` area, implemented by the game and reached through [`ServerHost`].
#[horizon_mod_api_macros::host_area]
pub trait NetworkServerHost {
    /// Side: server.
    /// Sends `packet` to that player's client as it is (`ServerPlayer.connection.send`), as a
    /// Paper plugin sends a packet through ProtocolLib or PacketEvents: a team, an objective, a
    /// score, a Tab-list entry the server doesn't keep. Only that player gets it, at once, or
    /// after the hook for the acting player. Nothing on the server changes, so the game may
    /// later send its own state over it. Errors: `NotConnected` for a stale, offline or fake
    /// player and on a client, `UnknownPacket` for a name that is not a clientbound play
    /// packet, `InvalidBody` for a body that does not decode whole (or a custom payload on an
    /// engine `horizon:` channel), and `PayloadTooLarge` for a custom payload past
    /// `ClientboundCustomPayloadPacket.MAX_PAYLOAD_SIZE`. A packet the client reads but
    /// cannot apply is the mod's to get right.
    #[access(WorldOnly)]
    fn player_send_packet(
        &mut self,
        player: PlayerRef,
        packet: ClientboundPlayPacket,
    ) -> Result<(), SendPacketError>;
}
