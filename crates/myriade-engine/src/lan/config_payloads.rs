//! Configuration I/O: the connection, plus the engine's own channels that every configuration
//! loop reads itself.
use horizon_protocol::{connection::Connection, packets::configuration as cfg};
use std::sync::Arc;

/// The engine's channels that every configuration loop reads itself, kept into play.
#[derive(Default)]
pub(super) struct EngineChannels {
    /// What the client announced in `minecraft:register`. Allocated on the first
    /// `minecraft:register` / `minecraft:unregister`: most connections never send one.
    pub announced: Option<Arc<super::announced_channels::AnnouncedChannels>>,
    /// Its `horizon:levels` answer.
    pub viewed_levels: super::viewed_levels::capability::Capability,
    /// Its `horizon:menu_extra` revision answer.
    pub menu_extra_rev: super::menu_extra_rev::Capability,
}
impl EngineChannels {
    /// Whether `payload` was one of these channels' and is taken, or the reason that closes the
    /// connection.
    pub fn receive(&mut self, payload: &cfg::CustomPayload) -> Result<bool, String> {
        // ServerCommonPacketListenerImpl hands the lists to the plugin messenger, in
        // configuration as in play: they stay on the connection.
        if super::announced_channels::is_list_channel(&payload.channel) {
            self.announced
                .get_or_insert_with(Arc::default)
                .apply(&payload.channel, &payload.data);
            return Ok(true);
        }
        if payload.channel == horizon_protocol::viewed_levels::CHANNEL {
            self.viewed_levels
                .receive(&payload.data)
                .map_err(|error| error.to_string())?;
            return Ok(true);
        }
        if payload.channel == horizon_protocol::menu_extra::CHANNEL {
            self.menu_extra_rev
                .receive(&payload.data)
                .map_err(|error| error.to_string())?;
            return Ok(true);
        }
        Ok(false)
    }
}

pub(super) struct ConfigIo<'a> {
    connection: &'a mut Connection,
    pub channels: EngineChannels,
    /// The client's build, once it names it (crate::build_mismatch).
    pub client_build: Option<String>,
}
impl<'a> ConfigIo<'a> {
    pub fn new(connection: &'a mut Connection) -> Self {
        Self {
            connection,
            channels: EngineChannels::default(),
            client_build: None,
        }
    }
    pub fn send(&mut self, body: &[u8]) -> horizon_protocol::Result<()> {
        self.connection.send(body)
    }
    /// Asks a Horizon-branded client how many levels besides its own it can view.
    pub fn ask_viewed_levels(&mut self) -> horizon_protocol::Result<()> {
        let query = self.channels.viewed_levels.query();
        self.send(&cfg::Clientbound::CustomPayload(query).encode())
    }
    /// Asks a Horizon-branded client which `horizon:menu_extra` layout it reads.
    pub fn ask_menu_extra_rev(&mut self) -> horizon_protocol::Result<()> {
        let query = self.channels.menu_extra_rev.query();
        self.send(&cfg::Clientbound::CustomPayload(query).encode())
    }
    pub fn application(
        &mut self,
        payload: cfg::CustomPayload,
    ) -> Result<Option<cfg::CustomPayload>, String> {
        match self.channels.receive(&payload) {
            Ok(true) => Ok(None),
            Ok(false) => Ok(Some(payload)),
            Err(error) => {
                self.disconnect(horizon_core::nbt::Tag::String(error.as_str().into()));
                Err(error)
            }
        }
    }
    pub fn disconnect(&mut self, reason: horizon_core::nbt::Tag) {
        let _ = self
            .connection
            .send(&cfg::Clientbound::Disconnect(cfg::Disconnect { reason }).encode());
    }
}
impl std::ops::Deref for ConfigIo<'_> {
    type Target = Connection;
    fn deref(&self) -> &Connection {
        self.connection
    }
}
impl std::ops::DerefMut for ConfigIo<'_> {
    fn deref_mut(&mut self) -> &mut Connection {
        self.connection
    }
}
