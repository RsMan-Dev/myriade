//! `RunOne[Stroll 2, DoNothing 1]` of an idle activity (`mobs/warden_brain.rs:317-318`, `:913`): a goal that holds MOVE and LOOK for a
//! stay drawn in `min..=max` ticks at its start (`DoNothing(Timed::new(30, 60))`). The framework has no weights: the choice against the
//! stroll is a chance per ask (`chance`, 1/3 for 2:1). Asks come from the wheel: one is armed after each refusal and after each stay.
//! DOUBT: the `Mob` chooses at each start of the activity, here at each ask; a stale stay deadline left by a stop is ignored (no ask).
use super::*;
use crate::{cow::*, goals::*};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// The shortest and longest stay, in ticks.
    pub min: i32,
    pub max: i32,
    /// The mean wait between two asks, in ticks (`idle_wait`).
    pub interval: i32,
    /// The chance per ask that the stay starts instead of the other goals of the same flags.
    pub chance: f64,
}

/// The state of the entry: its parameters, the stay drawn by `can_use` for `start`, the stay's end once started, and whether an ask is armed.
#[derive(Component, Clone, Copy, Debug)]
pub struct Pause {
    p: &'static Params,
    slot: u8,
    dur: i32,
    until: Option<u64>,
    armed: bool,
}

pub static MODULE: Module = Module {
    name: "timed_idle",
    flags: MOVE | LOOK,
    equip,
    arm,
    fired,
    can_use,
    start,
    stop,
    requires: &[Cap::Goals],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let p = params::<Params>(p);
    w.entity_mut(e).insert(Pause { p, slot: inst.slot, dur: 0, until: None, armed: false });
}
fn with<R>(w: &mut World, e: Entity, f: impl FnOnce(&mut Pause) -> R) -> Option<R> {
    w.get_mut::<Pause>(e).map(|mut s| f(&mut s))
}
/// Arms the next ask after a wait, unless one is armed.
fn arm(w: &mut World, e: Entity, inst: Inst) {
    let Some(s) = w.get::<Pause>(e).copied() else { return };
    if s.armed || s.slot != inst.slot {
        return;
    }
    let wait = with_rng(w, e, |r| idle_wait(r, s.p.interval));
    crate::goals::arm(w, e, inst.slot, wait);
    with(w, e, |s| s.armed = true);
}
/// The wheel's deadline: a stay under way ends when its time is up (a cut wait, `leave_block`'s rule, is armed again for the rest); otherwise
/// the deadline is an ask, unless it is stale (no ask armed).
fn fired(w: &mut World, e: Entity, inst: Inst) -> bool {
    let now = now_of(w, e);
    let Some(s) = w.get::<Pause>(e).copied() else { return false };
    if let Some(until) = s.until {
        if now < until {
            crate::goals::arm(w, e, inst.slot, (until - now).min(4000) as i32);
        } else {
            w.resource_mut::<Requests>().0.push(Req::End(e, inst.slot));
        }
        return false;
    }
    with(w, e, |s| s.armed = false);
    s.armed
}
fn can_use(w: &mut World, e: Entity, inst: Inst) -> bool {
    let Some(s) = w.get::<Pause>(e).copied() else { return false };
    if s.slot != inst.slot {
        return false;
    }
    if with_rng(w, e, |r| r.next_f64()) >= s.p.chance {
        arm(w, e, inst);
        return false;
    }
    let dur = with_rng(w, e, |r| s.p.min + r.next_int(s.p.max - s.p.min + 1));
    with(w, e, |s| s.dur = dur);
    true
}
fn start(w: &mut World, e: Entity, inst: Inst) {
    let now = now_of(w, e);
    let Some(dur) = with(w, e, |s| {
        s.until = Some(now + s.dur.max(0) as u64);
        s.dur
    }) else {
        return;
    };
    crate::goals::arm(w, e, inst.slot, dur);
}
fn stop(w: &mut World, e: Entity, inst: Inst) {
    with(w, e, |s| s.until = None);
    arm(w, e, inst);
}
