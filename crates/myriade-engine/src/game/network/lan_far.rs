//! Far terrain for a server's Horizon clients (`myriade_engine::lod_net`): a
//! `horizon:lod` request is checked against the player's limits and position,
//! then sampled from the level the player is in on the far terrain thread.
//! The replies come back here and leave with the tick's other packets. A
//! refused request is answered at once, so the client knows to ask again.
use super::{Dim, Game};
use myriade_engine::{
    lan::conn::PlayerSender,
    lod_net::{CHANNEL, Job, Verdict},
};
use horizon_protocol::{
    packets::{configuration::CustomPayload, play},
    text::translatable,
};

fn send(sender: &PlayerSender, data: Vec<u8>) {
    let packet = play::Clientbound::CustomPayload(CustomPayload {
        channel: CHANNEL.into(),
        data,
    });
    match packet.try_encode() {
        Ok(body) => sender.send(body),
        Err(error) => eprintln!("[lan] far terrain send refused: {error}"),
    }
}

impl Game<'_> {
    /// A `horizon:lod` payload from `conn`.
    pub(super) fn guest_far_terrain(&mut self, conn: u32, data: &[u8]) {
        let Some(host) = self.lan.as_mut() else {
            return;
        };
        let Some(far) = host.far.as_mut() else {
            return;
        };
        // Only the clients that heard the offer (lan::conn) may ask.
        let Some(guest) = host.hub.guest(conn).filter(|g| g.brand == "horizon") else {
            return;
        };
        // Players who left are forgotten once they could matter.
        if far.players() > host.hub.online_count() + 8 {
            let guests = &host.hub.guests;
            far.retain(|c| guests.iter().any(|g| g.conn == c));
        }
        let dim = Dim::from_id(&guest.dimension);
        let position = guest.position;
        let protected = host.hub.terrain_protected(conn, &guest.dimension);
        let source = if protected {
            None
        } else {
            self.lod_source_in(dim)
        };
        let Some(host) = self.lan.as_mut() else {
            return;
        };
        let Some(far) = host.far.as_mut() else {
            return;
        };
        let Some(guest) = host.hub.guest(conn) else {
            return;
        };
        let now = std::time::Instant::now();
        match far.request(conn, data, position, source.is_some(), now) {
            Verdict::Serve(request) => far.push(Job {
                conn,
                level: dim.id().to_owned(),
                source: source.expect("served when open"),
                request,
            }),
            Verdict::Refuse(reply) => send(&guest.sender, reply),
            Verdict::Ignore => {}
            Verdict::Kick => host.hub.kick(
                conn,
                translatable("multiplayer.disconnect.invalid_packet", &[]),
            ),
        }
    }
}
