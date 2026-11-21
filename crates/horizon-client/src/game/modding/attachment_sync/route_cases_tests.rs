//! More of step 4a's acceptance list over LAN TCP, from a server in the dedicated role: a
//! negotiated peer that does not see the holder is sent nothing, and a message keeps its
//! place among the player's held gameplay packets.
use super::transport_tests::{
    Route, VALUE, peer, set_level, store, sync_packets_within, values_until, wait_server_ticks,
};
use horizon_core::nbt::Tag;
use horizon_mod_runtime::attachments::{AttachmentSync, Holder};
use horizon_protocol::{
    attachment_sync,
    buf::{Codec, Ident, Reader},
    connection::Connection,
    packets::play,
};
use std::time::{Duration, Instant};

const NAME: &str = "game::attachment_sync::route_cases_tests::a_dedicated_server_sends_only_trackers_in_packet_order";

/// What a peer is sent, reduced to what this test orders: system chat text and the level's
/// synced values, until it has seen `count` of them.
#[derive(Debug, PartialEq)]
enum Seen {
    Chat(String),
    Sync(Holder, i32),
}
fn seen_until(c: &mut Connection, count: usize) -> Vec<Seen> {
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut seen = Vec::new();
    while seen.len() < count {
        assert!(Instant::now() < deadline, "only {seen:?}");
        let (id, body) = match c.recv() {
            Ok(packet) => packet,
            Err(error) if error.is_timed_out() => continue,
            Err(error) => panic!("peer receive failed: {error}"),
        };
        if id == horizon_protocol::ids::play::clientbound::CUSTOM_PAYLOAD {
            let mut reader = Reader::new(&body);
            if Ident::read(&mut reader).unwrap() == attachment_sync::CHANNEL {
                let message = attachment_sync::decode(reader.rest()).unwrap();
                for change in message
                    .changes
                    .iter()
                    .filter(|_| message.holder == Holder::Level)
                {
                    if let Some(Tag::Int(n)) = change
                        .value
                        .as_ref()
                        .and_then(|value| value.get(&"n".into()))
                    {
                        seen.push(Seen::Sync(message.holder, *n));
                    }
                }
                continue;
            }
        }
        match play::Clientbound::decode(id, &body).unwrap() {
            play::Clientbound::SystemChat(p) => {
                if let Tag::String(text) = p.content {
                    seen.push(Seen::Chat(text.to_string().unwrap_or_default()));
                }
            }
            play::Clientbound::KeepAlive(p) => {
                c.send(&play::Serverbound::KeepAlive(p).encode()).unwrap()
            }
            play::Clientbound::Disconnect(p) => panic!("disconnected: {p:?}"),
            _ => {}
        }
    }
    seen
}
