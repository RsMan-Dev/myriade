//! Titles, the action bar and the selected item name: the server sends them as packets.
use super::Game;
use myriade_engine::text::Text;

impl Game<'_> {
    /// `ServerPlayer.sendOverlayMessage`: a server client receives it as a packet.
    pub(super) fn send_overlay_message(&mut self, message: Text) {
        if self.lan_acting.is_some() {
            self.guest_packets
                .push(horizon_protocol::packets::play::Clientbound::SystemChat(
                    horizon_protocol::packets::play::SystemChat {
                        content: myriade_engine::lan::data::component_nbt(&message.component()),
                        overlay: true,
                    },
                ));
        }
    }
}
