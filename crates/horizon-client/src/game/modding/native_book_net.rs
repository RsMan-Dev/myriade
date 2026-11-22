//! Private custom-book synchronization, admitted by its own configuration capability.
use super::Game;
use horizon_client::recipe_book::{
    settings_codec,
    settings_sync::CHANNEL,
};
use horizon_protocol::packets::{configuration::CustomPayload, play::Clientbound as C};
use std::{
    collections::BTreeSet,
    sync::atomic::{AtomicU64, Ordering},
};
static EPOCH: AtomicU64 = AtomicU64::new(1);
fn registered() -> BTreeSet<String> {
    mod_api::RecipeBookTypeKey::registered()
        .into_iter()
        .map(|key| key.as_str().to_owned())
        .collect()
}
impl Game<'_> {

    pub(super) fn sync_replaced_book_settings(&mut self, uuid: u128) {
        let Some(host) = self.lan.as_ref() else {
            return;
        };
        let Some(guest) = host.hub.guests.iter().find(|guest| guest.uuid == uuid) else {
            return;
        };
        let Some(authority) = self
            .native_net
            .book_connections
            .get_mut(&guest.conn)
            .and_then(|connection| connection.authority.as_mut())
        else {
            return;
        };
        let Some(revision) = authority.revision.checked_add(1) else {
            return;
        };
        authority.revision = revision;
        if let Some(data) = settings_codec::encode(&authority.snapshot(&guest.recipe_book.custom)) {
            guest.send(&C::CustomPayload(CustomPayload {
                channel: CHANNEL.into(),
                data,
            }));
        }
    }

    pub(super) fn clear_guest_book_connection(&mut self, conn: u32) {
        self.guest_combat.remove(&conn);
        self.native_net.book_connections.remove(&conn);
        self.native_net.placement.servers.remove(&conn);
    }
    pub(super) fn notify_vanilla_book_settings(
        &mut self,
        uuid: u128,
        name: &str,
        book: i32,
        open: bool,
        filtering: bool,
    ) {
        let book = match book {
            0 => mod_api::RecipeBookIdentity::Crafting,
            1 => mod_api::RecipeBookIdentity::Furnace,
            2 => mod_api::RecipeBookIdentity::BlastFurnace,
            3 => mod_api::RecipeBookIdentity::Smoker,
            _ => return,
        };
        self.native_player_recipe_book_settings_change(uuid, name, book, open, filtering);
    }

    pub(super) fn sync_server_book_write(
        &mut self,
        player: mod_api::PlayerRef,
        book: &mod_api::RecipeBookTypeKey,
        flags: horizon_client::recipe_book::BookFlags,
    ) {
        let Some(host) = self.lan.as_ref() else {
            return;
        };
        let Some(guest) = host
            .hub
            .guests
            .iter()
            .find(|g| g.uuid == player.uuid() && g.placed)
        else {
            return;
        };
        let Some(authority) = self
            .native_net
            .book_connections
            .get_mut(&guest.conn)
            .and_then(|connection| connection.authority.as_mut())
        else {
            return;
        };
        if let Some(message) = authority.server_update(book.as_str(), flags)
            && let Some(data) = settings_codec::encode(&message)
        {
            guest.send(&C::CustomPayload(CustomPayload {
                channel: CHANNEL.into(),
                data,
            }));
        }
    }

    pub(super) fn guest_book_settings(&mut self, conn: u32, data: &[u8]) {
        let Some(host) = self.lan.as_mut() else {
            return;
        };
        if !host
            .hub
            .guest(conn)
            .is_some_and(|guest| guest.placed && guest.brand == "horizon")
        {
            return;
        }
        let connection = self.native_net.book_connections.entry(conn).or_default();
        if data.first() == Some(&settings_codec::PLACEMENT_TAG) {
            let frame = connection.receive_placement(data, std::time::Instant::now());
            if let Some(frame) = frame {
                self.guest_mod_book_placement(conn, frame);
            }
            return;
        }
        let changed = host
            .hub
            .guests
            .update_connection(conn, |guest| {
                let reply = connection.receive(
                    data,
                    &mut guest.recipe_book.custom,
                    registered,
                    || EPOCH.fetch_add(1, Ordering::Relaxed),
                    std::time::Instant::now(),
                )?;
                if let Some(data) = settings_codec::encode(&reply.message) {
                    guest.send(&C::CustomPayload(CustomPayload {
                        channel: CHANNEL.into(),
                        data,
                    }));
                }
                reply
                    .changed
                    .map(|(id, flags)| (guest.uuid, guest.name.clone(), id, flags))
            })
            .flatten();
        if let Some((uuid, name, id, flags)) = changed
            && let Some(key) = mod_api::RecipeBookTypeKey::registered()
                .into_iter()
                .find(|key| key.as_str() == id)
        {
            self.native_player_recipe_book_settings_change(
                uuid,
                &name,
                mod_api::RecipeBookIdentity::Custom(key),
                flags.open,
                flags.filtering,
            );
        }
    }
}
