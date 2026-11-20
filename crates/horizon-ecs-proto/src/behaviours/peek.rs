//! `peek`: an idle goal that opens the body's lid for a while. Chance `one_in` per ask (when no target and the spot still holds the body),
//! the raw peek goes to `open` for `ticks x (1 + nextInt(spread))` ticks, halved by `reduced`; it shuts when it stops unless a target came.
//! A target ends it at once. The clock is a count down through `tick_state`.
use super::*;
use crate::behaviours::attached_face::{Sounds, set_peek};
use crate::behaviours::cat_state::reduced;
use crate::goals::*;
use crate::targeting::Target;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub one_in: i32,
    pub open: i32,
    pub ticks: i32,
    pub spread: i32,
    pub sounds: Sounds,
    pub covered: bool,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct Peek {
    pub slot: u8,
    pub time: i32,
    pub cfg: &'static Params,
}
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Peeking;

pub static MODULE: Module = Module {
    name: "peek",
    flags: 0,
    equip,
    arm,
    can_use,
    start,
    stop,
    tick: Some(tick),
    requires: &[Cap::Body, Cap::Goals, Cap::Targets],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Peek { slot: inst.slot, time: 0, cfg: params::<Params>(p) });
}
fn arm(w: &mut World, e: Entity, inst: Inst) {
    crate::goals::arm(w, e, inst.slot, 1);
}
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    let one_in = w.get::<Peek>(e).unwrap().cfg.one_in;
    w.get::<Target>(e).is_none() && with_rng(w, e, |r| r.next_int(reduced(one_in))) == 0 && crate::behaviours::teleport_on::can_stay(w, e)
}
fn start(w: &mut World, e: Entity, _: Inst) {
    let p = w.get::<Peek>(e).unwrap().cfg;
    let time = reduced(p.ticks * (1 + with_rng(w, e, |r| r.next_int(p.spread))));
    w.get_mut::<Peek>(e).unwrap().time = time;
    w.entity_mut(e).insert(Peeking);
    set_peek(w, e, p.open, &p.sounds, p.covered);
}
fn stop(w: &mut World, e: Entity, _: Inst) {
    let p = w.get::<Peek>(e).unwrap().cfg;
    w.entity_mut(e).remove::<Peeking>();
    if w.get::<Target>(e).is_none() {
        set_peek(w, e, 0, &p.sounds, p.covered);
    }
}
fn tick(w: &mut World) {
    crate::goals::tick_state(w, run);
    crate::goals::tick_state(w, crate::behaviours::attached_face::ease);
}
fn run(q: Query<(Entity, &Peek, Option<&Target>), With<Peeking>>, mut cmd: Commands) {
    for (e, p, target) in &q {
        let slot = p.slot;
        let targeted = target.is_some();
        cmd.queue(move |w: &mut World| {
            let done = match w.get_mut::<Peek>(e) {
                Some(mut p) if !targeted => {
                    p.time -= 1;
                    p.time <= 0
                }
                _ => true,
            };
            if done {
                w.resource_mut::<Requests>().0.push(Req::End(e, slot));
            }
        });
    }
}
