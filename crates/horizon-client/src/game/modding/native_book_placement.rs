//! Placement transport state belongs to the actual connection and open menu.
use super::*;
use horizon_client::recipe_book::{
    placement_sync as sync, settings_codec,
    settings_sync::{CHANNEL, Message},
};
use horizon_protocol::packets::{configuration::CustomPayload, play::Clientbound as C};
use std::collections::{BTreeMap, HashMap};

// no wire: live connection authority and validated engine presentation caches.
#[derive(Default)]
pub(super) struct State {
    pub(super) client: sync::Client,
    pub(super) servers: HashMap<u32, Server>,
    pub(super) view: Option<View>,
    pub(super) deadline: Option<std::time::Instant>,
}
// no wire: source ordinals actually published to this connection.
pub(super) struct Server {
    pub(super) peer: sync::Authority,
    pub(super) displays: BTreeMap<String, u32>,
}
impl Server {
    pub(super) fn new(epoch: u64) -> Self {
        Self {
            peer: sync::Authority::new(epoch),
            displays: BTreeMap::new(),
        }
    }
    pub(super) fn clear(&mut self) {
        self.peer.clear();
        self.displays.clear();
    }
}
// no wire: an installed, validated catalogue, not an untrusted transport body.
pub(super) struct View {}

impl Game<'_> {
    pub(super) fn send_mod_book_placement(&mut self, frame: sync::Frame) {
        if settings_codec::encode(&Message::Placement(frame)).is_some() {
            self.native_net.placement.deadline =
                Some(std::time::Instant::now() + std::time::Duration::from_secs(30));
        }
    }
    pub(super) fn send_guest_mod_book_placement(&self, conn: u32, frame: sync::Frame) {
        if let Some(guest) = self.lan.as_ref().and_then(|host| host.hub.guest(conn))
            && let Some(data) = settings_codec::encode(&Message::Placement(frame))
        {
            guest.send(&C::CustomPayload(CustomPayload {
                channel: CHANNEL.into(),
                data,
            }));
        }
    }
    pub(super) fn clear_mod_book_placement_screen(&mut self) {
        if self.native_net.placement.client.window().is_some() {
            self.native_net.placement.deadline = None;
        }
        self.native_net.placement.client.close();
        self.native_net.placement.view = None;
    }
    pub(super) fn ensure_mod_book_catalog(&mut self) {
        let Some(window) = self.native_book_placement_window() else {
            return;
        };
        if self.native_net.placement.client.window() == Some(&window) {
            return;
        }
        if let Some(frame) = self.native_net.placement.client.open(window) {
            self.native_net.placement.view = None;
            self.send_mod_book_placement(frame);
        }
    }
}
