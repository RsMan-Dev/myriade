//! The state the three panda goals share (`panda.rs:44-62`, `105-120`, `186-199`): the flags (sneeze, roll, sit, on its
//! back), the eating counter `acts` reads, the gene. Not a module: data and the helpers that read it, nothing else.
use super::*;
use crate::cow::{geometric, GrowAt};

pub const FLAG_SNEEZE: u8 = 2;
pub const FLAG_ROLL: u8 = 4;
pub const FLAG_SIT: u8 = 8;
pub const FLAG_ON_BACK: u8 = 16;

/// The gene a panda shows (`Panda.getVariant`, `panda.rs:27-31`). A panda without one is `Normal`.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Gene {
    #[default]
    Normal,
    Lazy,
    Worried,
    Playful,
    Weak,
    Aggressive,
}

/// `DATA_ID_FLAGS` and `EAT_COUNTER`: what `acts` reads. Written by the goals that set a flag.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct PandaState {
    pub flags: u8,
    pub eat: i32,
}

/// Creates the shared state if it is missing (each goal calls it at its creation: idempotent).
pub fn ensure(w: &mut World, e: Entity) {
    if w.get::<PandaState>(e).is_none() {
        w.entity_mut(e).insert(PandaState::default());
    }
}
pub fn flag(w: &World, e: Entity, f: u8) -> bool {
    w.get::<PandaState>(e).is_some_and(|s| s.flags & f != 0)
}
pub fn set_flag(w: &mut World, e: Entity, f: u8, on: bool) {
    ensure(w, e);
    let mut s = w.get_mut::<PandaState>(e).unwrap();
    if on {
        s.flags |= f;
    } else {
        s.flags &= !f;
    }
}
/// A cub: it has the `GrowAt` component until it grows up.
pub fn is_baby(w: &World, e: Entity) -> bool {
    w.get::<GrowAt>(e).is_some()
}
pub fn gene(w: &World, e: Entity) -> Gene {
    w.get::<Gene>(e).copied().unwrap_or_default()
}
/// `panda.rs:122`: the aggressive gene (`hurt_by.alert_when` of the other pandas).
pub fn is_aggressive(w: &World, e: Entity) -> bool {
    gene(w, e) == Gene::Aggressive
}
/// `Panda.isScared` needs a thunderstorm, which the world does not have yet: no storm (DOUTE in the notes).
fn thundering(_: &World) -> bool {
    false
}
/// `Panda.canPerformAction` (`panda.rs:197-199`): not on its back, rolling or sitting, not eating, not scared.
pub fn acts(w: &World, e: Entity) -> bool {
    let s = w.get::<PandaState>(e).copied().unwrap_or_default();
    let scared = gene(w, e) == Gene::Worried && thundering(w);
    s.flags & (FLAG_ON_BACK | FLAG_ROLL | FLAG_SIT) == 0 && !scared && s.eat <= 0
}
/// Asks until the first success of a 1-in-`n` ask, the first ask counting as 1: `geometric` gives the failures, so
/// `geometric + 1`. The goal asks every 2 ticks (the selector's phase), hence the wait is `2 * asks_until`.
pub fn asks_until(w: &mut World, e: Entity, n: i32) -> i32 {
    crate::goals::with_rng(w, e, |r| geometric(r, n)) + 1
}
/// Arms the goal's deadline at `at` (the entity's own time). The wheel holds at most `WHEEL_MAX` ticks: a later deadline
/// comes early and the goal's `fired` arms the rest.
pub fn arm_to(w: &mut World, e: Entity, slot: u8, at: u64) {
    let now = crate::cow::now_of(w, e);
    crate::goals::arm(w, e, slot, at.saturating_sub(now).min(i32::MAX as u64) as i32);
}
