//! `player_disconnect` from a play payload handler, over real TCP: the reason reaches the
//! right client once, a second call (from `player_kicked`) is refused, and the hub's poll
//! tells `player_logged_out` exactly once per player.
use super::sender_tests::connect_peer;
use horizon_protocol::{
    connection::Connection,
    packets::{configuration as cfg, play},
};
use mod_api::{
    HandlerThread, Hooks, IPayloadContext, IPayloadHandler, ModEntry, PayloadBytes, PayloadId,
    PlayerKickedEvent, PlayerLeavingEvent, PlayerLoggedOutEvent, Registrar, ServerHost, Text,
    text::TextFormError,
};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

const NAME: &str = "game::payload_transport::disconnect_tests::a_payload_handler_disconnects_once_and_the_poll_logs_out_once";
const CHANNEL: &str = "disconnect_probe:kick";
const PLAYERS: [&str; 2] = ["SenderOne", "SenderTwo"];
const REASONS: [&str; 2] = [
    "SenderOne left by its own payload",
    "SenderTwo kicked by SenderOne",
];

/// One handled payload: (sender, acting player, target, answer). A main-thread payload
/// handler runs outside any guest turn: no acting player, both reached as connected records.
type Handled = (u128, Option<u128>, u128, Result<bool, TextFormError>);

#[derive(Default)]
struct Probe {
    handled: Mutex<Vec<Handled>>,
    kicked: Mutex<Vec<(u128, String)>>,
    second: Mutex<Vec<Option<Result<bool, TextFormError>>>>,
    logged_out: Mutex<Vec<u128>>,
    /// (player, reason, still in `server_players()`) when `player_leaving` was heard.
    leaving: Mutex<Vec<(u128, String, bool)>>,
    /// Whether the player was still in `server_players()` at `player_logged_out`.
    listed_at_logout: Mutex<Vec<bool>>,
}
impl IPayloadHandler for Probe {
    /// The payload's one byte names the player to disconnect.
    fn handle(
        &self,
        bytes: PayloadBytes,
        context: &mut dyn IPayloadContext,
    ) -> Result<(), mod_api::PayloadHandleError> {
        let sender = context.player().unwrap();
        let host = context.server_host().unwrap();
        let index = usize::from(bytes.0[0]);
        let target = host.players().player_by_name(PLAYERS[index]).unwrap();
        let acting = host.players().acting_player().map(|p| p.uuid());
        let answer = target.disconnect(host, Text::literal(REASONS[index]));
        self.handled
            .lock()
            .unwrap()
            .push((sender.uuid(), acting, target.uuid(), answer));
        Ok(())
    }
}
struct Registration(Arc<Probe>);
#[mod_api::__subscriptions]
impl Hooks for Registration {
    fn register(&self, registrar: &mut dyn Registrar, _: &mod_api::RegistrationContext) {
        registrar
            .payload_handlers()
            .registrar("1")
            .executes_on(HandlerThread::Main)
            .play_bidirectional(
                PayloadId(CHANNEL.into()),
                self.0.clone(),
                Some(self.0.clone()),
            );
    }
    fn player_kicked(&self, host: &mut dyn ServerHost, event: &mut PlayerKickedEvent<'_>) {
        let second = host
            .player(event.player)
            .map(|player| player.disconnect(host, Text::literal("Second reason")));
        self.0.second.lock().unwrap().push(second);
        self.0
            .kicked
            .lock()
            .unwrap()
            .push((event.player, event.reason.to_string()));
    }
    fn player_leaving(&self, host: &mut dyn ServerHost, event: &mut PlayerLeavingEvent<'_>) {
        let listed = |host: &dyn ServerHost, uuid| {
            host.players()
                .server_players()
                .iter()
                .any(|p| p.uuid() == uuid)
        };
        let here = listed(host, event.player);
        self.0
            .leaving
            .lock()
            .unwrap()
            .push((event.player, event.reason.to_string(), here));
    }
    fn player_logged_out(&self, host: &mut dyn ServerHost, event: &mut PlayerLoggedOutEvent<'_>) {
        let here = host
            .players()
            .server_players()
            .iter()
            .any(|p| p.uuid() == event.player);
        self.0.listed_at_logout.lock().unwrap().push(here);
        self.0.logged_out.lock().unwrap().push(event.player);
    }
}

/// The disconnect reasons a peer reads (as plain text) until its connection closes.
fn reasons_until_closed(c: &mut Connection) -> Vec<String> {
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut reasons = Vec::new();
    while let Ok((id, body)) = c.recv() {
        assert!(Instant::now() < deadline, "the server never closed it");
        match play::Clientbound::decode(id, &body) {
            Ok(play::Clientbound::Disconnect(p)) => reasons.push(horizon_protocol::text::plain(
                &p.reason,
                &Default::default(),
            )),
            Ok(play::Clientbound::KeepAlive(p)) => {
                drop(c.send(&play::Serverbound::KeepAlive(p).encode()))
            }
            _ => {}
        }
    }
    reasons
}

fn wait_for(probe: &Probe, logged_out: usize) {
    let deadline = Instant::now() + Duration::from_secs(60);
    while probe.logged_out.lock().unwrap().len() < logged_out {
        assert!(Instant::now() < deadline, "player_logged_out never came");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn send(c: &mut Connection, target: u8) {
    c.send(
        &play::Serverbound::CustomPayload(cfg::CustomPayload {
            channel: CHANNEL.into(),
            data: vec![target],
        })
        .encode(),
    )
    .unwrap();
}

