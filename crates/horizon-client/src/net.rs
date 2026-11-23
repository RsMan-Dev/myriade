//! What the server's receiving paths share with the game: the events they pass on
//! (`Event`), the bounded ingress queue, the writer that sends a server's packets,
//! and the language its components resolve with.
pub mod ingress;
pub(crate) mod queued_writer;

pub use queued_writer::PreparedPayload;

use horizon_protocol::{
    packets::{configuration as cfg, play},
    registry::Dynamic,
    text::Language,
};
use std::sync::Arc;

/// What the network thread reports to the game.
pub enum Event {
    /// Progress text for the connecting screen.
    Status(String),
    /// The server's `minecraft:brand` custom payload.
    Brand(String),
    /// The server runs mods' quiet commands (`horizon_protocol::quiet`).
    QuietCommands,
    /// The server supports private custom recipe-book synchronization.
    CustomBooks,
    /// The server samples far terrain for this client (`lod_net`).
    FarTerrain(crate::lod_net::Offer),
    /// ClientConfigurationPacketListenerImpl.handleEnabledFeatures.
    Features(Vec<String>),
    /// Configuration finished: the play state begins with these registries.
    Joined {
        registries: Arc<Dynamic>,
        uuid: u128,
        name: String,
    },
    Packet(Box<play::Clientbound>),
    /// Registered application bytes, owned until the receiving root transaction ends.
    RawPayload(Box<crate::raw_payloads::Incoming>),
    /// A packet that failed to decode (skipped, like vanilla's skippable packets).
    DecodeError(String),
    /// The server sent us back to configuration (`ClientboundStartConfigurationPacket`):
    /// the game shows `ServerReconfigScreen` until the server logs it in again.
    Reconfiguring,
    /// The server sent us back to configuration and finished it again.
    Reconfigured(Arc<Dynamic>),
    /// ClientboundResourcePackPushPacket, in configuration or in play.
    PackPush(cfg::ResourcePackPush),
    /// ClientboundResourcePackPopPacket: one pack, or every pack.
    PackPop(Option<u128>),
    /// `show_dialog` in configuration: the dialog itself.
    ShowDialog(horizon_core::nbt::Tag),
    /// `clear_dialog` in configuration.
    ClearDialog,
    /// ClientboundCodeOfConductPacket: the game shows it and answers with
    /// `Session::accept_code_of_conduct` or a disconnect; until then the
    /// server waits in configuration.
    CodeOfConduct(String),
    /// `ClientboundTransferPacket`: the server sent us to another address.
    /// The connection ends right after; the game reconnects there with the
    /// stored cookies.
    Transfer {
        host: String,
        port: i32,
    },
    /// `ClientboundServerLinksPacket`: the links the pause menu's "Server
    /// Links..." lists (ClientCommonPacketListenerImpl.handleServerLinks).
    ServerLinks(Vec<cfg::ServerLink>),
    /// The connection ended. `from_server`: the server gave the reason (or
    /// the connection was refused for good), rather than the connection
    /// failing (auto-reconnect retries only failures and shutdowns).
    Disconnected {
        reason: String,
        /// The reason's translation key, when it had one
        /// (`reconnect::allowed` classifies by it).
        key: Option<String>,
        from_server: bool,
    },
}

impl Event {
    fn ingress_charge(&self) -> usize {
        use horizon_protocol::retained::OwnedHeap;
        use std::mem::{align_of, size_of};
        let dynamic = |value: &Dynamic| {
            size_of::<Dynamic>() + 2 * size_of::<usize>() + align_of::<Dynamic>() - 1
                + value.fresh_owned_heap_bytes()
        };
        let owned = match self {
            Self::Status(s) | Self::Brand(s) | Self::DecodeError(s) | Self::CodeOfConduct(s) => {
                s.capacity()
            }
            Self::QuietCommands
            | Self::CustomBooks
            | Self::Reconfiguring
            | Self::ClearDialog
            | Self::PackPop(_) => 0,
            Self::FarTerrain(crate::lod_net::Offer {
                distance: _,
                in_flight: _,
                per_second: _,
            }) => 0,
            Self::Features(values) => values.owned_heap_bytes(),
            // configure only inserts/replaces before freezing these snapshots.
            Self::Joined {
                registries,
                uuid: _,
                name,
            } => dynamic(registries).saturating_add(name.capacity()),
            Self::Reconfigured(registries) => dynamic(registries),
            Self::Packet(packet) => {
                size_of::<play::Clientbound>().saturating_add(packet.as_ref().owned_heap_bytes())
            }
            Self::RawPayload(incoming) => size_of::<crate::raw_payloads::Incoming>()
                .saturating_add(incoming.owned_heap_bytes()),
            Self::PackPush(packet) => packet.owned_heap_bytes(),
            Self::ShowDialog(tag) => tag.owned_heap_bytes(),
            Self::Transfer { host, port: _ } => host.capacity(),
            Self::ServerLinks(links) => links.owned_heap_bytes(),
            Self::Disconnected {
                reason,
                key,
                from_server: _,
            } => reason.capacity().saturating_add(key.owned_heap_bytes()),
        };
        // Layout-derived allowance for a frozen receiving vector, actor backlog,
        // decoder marker and retained receipt, including vector spare capacity.
        // Raw application buffers themselves are never multiplied.
        owned.saturating_add(8 * size_of::<(Self, ingress::ReceiveLease<Self>, usize, usize)>())
    }
}

/// The language a session resolves server-sent components with: the selected
/// one as `lang` loaded it (`en_us` under it, packs included), so a menu title
/// the server sends as a key follows the language screen. A process with no
/// language set (a dedicated server's log) reads en_us.json.
pub fn load_language(resources: &std::path::Path) -> Language {
    if let Some(lang) = crate::lang::current() {
        return lang.map().clone();
    }
    // No language screen ran (a dedicated server's log): en_us, with
    // Horizon's own strings included.
    horizon_client_core::lang::Language::load(&[&resources.to_path_buf()], "en_us")
        .map()
        .clone()
}
