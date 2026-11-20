//! `Croak` (`mobs/frog.rs:237-270`): a standing frog on the ground, out of fluids and with no walk to make, croaks for `ticks` (60) own ticks
//! once it passes the draw `nextInt(4) != 0` that each ask makes. Asks come by events, not by polling: at creation, when a goal ends
//! (`after_change`), on landing and on leaving a fluid; a draw that fails asks again 2 ticks later (the selector's rhythm). The goal ends by
//! one deadline. The croaking pose is the marker `Croaking` (no pose or animation in the prototype: left out, DOUTE of the fiche).
use super::*;
use crate::cow::*;
use crate::goals::*;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// Own ticks of one croak (`CROAK_TICKS`).
    pub ticks: i32,
    /// The ask passes when `nextInt(skip) != 0`.
    pub skip: i32,
}
impl Params {
    pub const DEFAULT: Self = Self { ticks: 60, skip: 4 };
}
#[derive(Component, Clone, Copy, Debug)]
pub struct Croaker {
    pub slot: u8,
    pub ticks: i32,
    pub skip: i32,
    /// Own time at which the running croak ends.
    pub end: u64,
}
/// The frog croaks (its pose): the goal's marker.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Croaking;

pub static MODULE: Module = Module {
    name: "croak",
    flags: MOVE,
    equip,
    arm,
    fired,
    can_use,
    start,
    stop,
    after_change: Some(after_change),
    tick: Some(tick),
    requires: &[Cap::Body, Cap::Goals, Cap::Locomotion],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let p = *params::<Params>(p);
    w.entity_mut(e).insert(Croaker { slot: inst.slot, ticks: p.ticks, skip: p.skip, end: 0 });
}
fn arm(w: &mut World, e: Entity, inst: Inst) {
    crate::goals::arm(w, e, inst.slot, 2);
}
/// The conditions of `checkExtraStartConditions` and `canUse` but the draw: standing (not already croaking), on the ground, in no fluid,
/// no walk to make.
fn free(w: &World, e: Entity) -> bool {
    let on_ground = w.get::<Contact>(e).is_some_and(|c| c.on_ground);
    let in_fluid = w.get::<FluidState>(e).is_some_and(|f| f.in_water || f.in_lava);
    on_ground && !in_fluid && w.get::<Following>(e).is_none() && w.get::<NeedsPath>(e).is_none() && w.get::<Croaking>(e).is_none()
}
/// A deadline of the entry: the end of the croak, or a request.
fn fired(w: &mut World, e: Entity, inst: Inst) -> bool {
    // Another deadline (a request) may come up during the croak: only its own end ends it.
    if w.get::<Croaking>(e).is_some() {
        if w.get::<Croaker>(e).is_some_and(|c| now_of(w, e) >= c.end) {
            w.resource_mut::<Requests>().0.push(Req::End(e, inst.slot));
        }
        return false;
    }
    true
}
fn can_use(w: &mut World, e: Entity, inst: Inst) -> bool {
    if !free(w, e) {
        return false;
    }
    let skip = w.get::<Croaker>(e).map_or(4, |c| c.skip);
    if with_rng(w, e, |r| r.next_int(skip)) != 0 {
        return true;
    }
    crate::goals::arm(w, e, inst.slot, 2);
    false
}
fn start(w: &mut World, e: Entity, inst: Inst) {
    let ticks = w.get::<Croaker>(e).map_or(60, |c| c.ticks);
    let end = now_of(w, e) + ticks as u64;
    if let Some(mut c) = w.get_mut::<Croaker>(e) {
        c.end = end;
    }
    w.entity_mut(e).insert(Croaking);
    crate::goals::arm(w, e, inst.slot, ticks);
}
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<Croaking>();
}
/// A goal of the frog ended: the croak is asked again (its flag may be free).
fn after_change(w: &mut World, e: Entity, c: Change) {
    if c == Change::Ended && w.get::<Croaking>(e).is_none() {
        arm_module(w, e, &MODULE, 2);
    }
}
fn tick(w: &mut World) {
    tick_state(w, events_run);
}
/// The conditions a deadline does not see come true: the frog lands or leaves a fluid.
fn events_run(ev: Res<Events>, mut reqs: ResMut<Requests>, q: Query<&Croaker>) {
    for e in ev.landed.iter().chain(ev.left_fluid.iter()) {
        if q.get(e).is_ok() {
            reqs.0.push(Req::Call(e, ask));
        }
    }
}
fn ask(w: &mut World, e: Entity) {
    if w.get::<Croaking>(e).is_none() {
        arm_module(w, e, &MODULE, 2);
    }
}
