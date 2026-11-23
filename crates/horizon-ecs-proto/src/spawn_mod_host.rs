//! The mods' spawn hooks of the natural pass: `MobSpawnEvent.SpawnPlacementCheck` (the predicate of a start), `PositionCheck` (the mob's
//! checks) and `FinalizeSpawn` (the finalization of a birth). The game answers them for one step, lent in `SpawnModCalls` as the mod goals
//! are (`ModGoalCalls`). With no host lent (no mod listens) the pass calls nothing and draws the same numbers.
use crate::spawn_finalize::Group;
use crate::spawn_reason::SpawnReason;
use bevy_ecs::prelude::*;

/// What a `FinalizeSpawn` listener leaves a birth with: finalized as vanilla does (with the special multiplier a listener set, if any),
/// finalization skipped, or the birth cancelled.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Verdict {
    Done(Option<f32>),
    Skipped,
    Cancelled,
}

/// The game's side of the spawn hooks, asked by the natural pass. The kind is the species' name, the position the start's.
pub trait SpawnModHost {
    /// `SpawnPlacementCheck`: the answer for `kind` at `pos`; `default` is vanilla's predicate.
    fn placement_check(&self, kind: &str, pos: [i32; 3], reason: SpawnReason, default: bool) -> bool;
    /// `PositionCheck`: the answer for a fresh `kind` at `pos`; None when vanilla's checks stand.
    fn position_check(&self, kind: &str, pos: [f64; 3], reason: SpawnReason) -> Option<bool>;
    /// `FinalizeSpawn`: heard before the birth is finalized; `group` is the pack's group data, which the listeners may change.
    fn finalize_spawn(&self, kind: &str, pos: [f64; 3], reason: SpawnReason, group: &mut Option<Group>) -> Verdict;
}

/// The host lent for one step, its lifetime erased: `SpawnModCalls` holds it only while `ecs::step_with` runs.
#[derive(Clone, Copy)]
pub struct LentSpawnHost(&'static dyn SpawnModHost, std::thread::ThreadId);
// SAFETY: it is only read by the natural pass, which runs with the whole world (`&mut World`) on the thread that lent it (`get` checks it
// in debug builds).
unsafe impl Send for LentSpawnHost {}
unsafe impl Sync for LentSpawnHost {}
impl LentSpawnHost {
    pub fn lend(host: &dyn SpawnModHost) -> LentSpawnHost {
        // SAFETY: the only holder is `SpawnModCalls`, emptied by `step_with` before `host`'s borrow ends, panics included.
        LentSpawnHost(unsafe { std::mem::transmute::<&dyn SpawnModHost, &'static dyn SpawnModHost>(host) }, std::thread::current().id())
    }
    fn get(self) -> &'static dyn SpawnModHost {
        debug_assert_eq!(self.1, std::thread::current().id());
        self.0
    }
}
/// The spawn hooks of the step; None when no mod listens (the pass then asks nothing).
#[derive(Resource, Default)]
pub struct SpawnModCalls(pub Option<LentSpawnHost>);

/// The host lent for the current step, if any.
pub fn lent(w: &World) -> Option<&'static dyn SpawnModHost> {
    w.get_resource::<SpawnModCalls>().and_then(|c| c.0).map(LentSpawnHost::get)
}
