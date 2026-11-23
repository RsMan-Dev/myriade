//! PURPOSE: a general entity's mod tick, asked for by the ECS (`GeneralEvent::ModTick`): its Pre, the tick hook of its type and its Post.
//! WHY: the ECS wakes a general entity only when a mod asked for its tick (a subscription or a deadline), so the host no longer walks
//! every row of the level each tick. A canceled Pre skips the tick and the Post, as the row walk did.
//! REFS: vanilla `ServerLevel.tickNonPassenger` (entity `tick()` then its passengers); `game/modding/native_ticks.rs`;
//! `game/native_hooks/general_entity_tick.rs`.
//! LOGS: a tick whose writes were rolled back, after a wasm failure or a native panic.
//! READS-BY: `game/ecs/general.rs`. ENDS-WHEN: the tick is dispatched, or its Pre canceled it.

use super::super::Game;
use super::general_type_catalog;
use mod_api::EntityRef;

impl Game<'_> {
    /// The mod tick of general entity `id` of registry slot `slot`: its Pre, then the tick and the Post unless the Pre canceled it.
    pub(in crate::game) fn general_mod_tick(&mut self, id: u32, slot: u16) {
        let kind = general_type_catalog().key(usize::from(slot));
        let mut round = self.native_tick_round();
        if self.native_tick_pre_one(&mut round, id, kind) {
            if let Some(epoch) = self.native_lifecycle.epoch() {
                self.dispatch_general_tick(kind, epoch, id);
            }
            self.native_tick_post_one(&round, id, kind);
        }
        // The round took the tick buffer; the batch end (nothing queued here) gives it back.
        self.native_tick_post_batch(round);
    }

    /// One row's subscribed behavior, contained: the dispatch runs in a
    /// transaction, so a native panic unwinding out of it (caught here, as
    /// the mob tick's is) or a wasm trap inside it (a wasm dispatch fails
    /// without unwinding) rolls back every write the row's parts made —
    /// their counters and listener edits — and nothing leaves the row. The
    /// next entity ticks as if nothing happened; a panicking subscriber is
    /// reported, not disabled.
    fn dispatch_general_tick(&mut self, kind: &str, epoch: mod_api::WorldEpoch, id: u32) {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            mod_api::run_transaction(self, |game, _| {
                let before = game.wasm_dispatch_failures.get();
                game.native_general_entity_tick(kind, EntityRef::new(epoch, id));
                (game.wasm_dispatch_failures.get() == before)
                    .then_some(())
                    .ok_or(())
            })
        }));
        match outcome {
            Ok(Ok(())) => {}
            // A wasm trap: the runtime poisoned the caller's group and told
            // the log; the transaction above aborted with this refusal.
            Ok(Err(())) => {
                log::error!(
                    "general entity {kind} #{id}: its tick's writes were rolled back after a wasm dispatch failure"
                );
            }
            // A native panic: the unwind aborted the transaction as it
            // passed, and the thread-local mod attribution named the part.
            Err(payload) => {
                let panic = payload
                    .downcast_ref::<&str>()
                    .map(|text| (*text).to_owned())
                    .or_else(|| payload.downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| "box".to_owned());
                log::error!(
                    "general entity {kind} #{id}: its tick's writes were rolled back after a native mod panicked: {panic}"
                );
            }
        }
    }
}
