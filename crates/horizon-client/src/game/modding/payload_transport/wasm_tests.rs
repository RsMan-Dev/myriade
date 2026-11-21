//! Actual compiled guest -> executor -> Game root proof. No substitute context.
use super::*;
use mod_api::{Hooks, ModEntry, Registrar};
use std::sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering},
};
use std::time::{Duration, Instant};
static ACTIVE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static STAGED_REPLIES: AtomicUsize = AtomicUsize::new(0);
static PUBLICATIONS: AtomicUsize = AtomicUsize::new(0);
static RECEIPTS: AtomicUsize = AtomicUsize::new(0);
static ROOTS: Mutex<Vec<(f32, f32, bool)>> = Mutex::new(Vec::new());
const TEST: &str = "game::payload_transport::wasm_tests::compiled_guest_runs_in_actual_game_roots";

pub(super) fn staged_reply(server_reply: bool) {
    if server_reply && ACTIVE.load(Ordering::SeqCst) {
        STAGED_REPLIES.fetch_add(1, Ordering::SeqCst);
    }
}
pub(super) fn publication(server_reply: bool) {
    if server_reply && ACTIVE.load(Ordering::SeqCst) {
        PUBLICATIONS.fetch_add(1, Ordering::SeqCst);
    }
}
pub(super) fn health(game: &Game<'_>, endpoint: &Endpoint, id: &PayloadId) -> Option<f32> {
    if endpoint.side() != Side::Server
        || !matches!(
            id.0.as_str(),
            "wasm_fixture:game_main" | "wasm_fixture:game_network"
        )
        || !ACTIVE.load(Ordering::SeqCst)
    {
        return None;
    }
    let player = endpoint.transport().player()?;
    let host: &dyn mod_api::ServerHost = game;
    mod_api::LivingRef::new(player.entity()).health(host)
}
pub(super) fn observe(
    before: Option<f32>,
    game: &Game<'_>,
    endpoint: &Endpoint,
    id: &PayloadId,
    commit: bool,
) {
    if let (Some(before), Some(after)) = (before, health(game, endpoint, id)) {
        ROOTS.lock().unwrap().push((before, after, commit));
    }
}
struct Receipt;
impl mod_api::IPayloadHandler for Receipt {
    fn handle(
        &self,
        bytes: PayloadBytes,
        context: &mut dyn IPayloadContext,
    ) -> Result<(), mod_api::PayloadHandleError> {
        RECEIPTS.fetch_add(1, Ordering::SeqCst);
        assert_eq!(context.flow(), PayloadDirection::ToClient);
        assert_eq!(bytes.0.len(), 1 << 20);
        for (i, byte) in bytes.0.iter().enumerate() {
            assert_eq!(*byte, ((i * 31 + 7) % 251) as u8);
        }
        Ok(())
    }
}
struct Native;
#[mod_api::__subscriptions]
impl Hooks for Native {
    fn register(&self, registrar: &mut dyn Registrar, _: &mod_api::RegistrationContext) {
        registrar.payload_handlers().registrar("1").play_to_client(
            PayloadId("raw_probe:receipt".into()),
            Some(Arc::new(Receipt)),
        );
    }
}
