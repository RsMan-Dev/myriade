//! `RandomLookAroundGoal`: now and then (one chance in `period` per ask) look in a random direction for 20 to 40 ticks.
use super::*;
use crate::rules::random_look::{scan_dir, scan_until};
use crate::{cow::*, goals::*};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub period: i32,
    /// The brain's `RandomLookAround` cooldown: the next look after a tick drawn in `min..=max` (`(150, 250)`); none: `period`.
    pub interval: Option<(i32, i32)>,
}
impl Params {
    pub const DEFAULT: Self = Self { period: 50, interval: None };
}
/// The ask period; `armed`: its deadline is in the wheel.
#[derive(Component, Clone, Copy, Debug)]
pub struct Scan {
    pub period: i32,
    pub interval: Option<(i32, i32)>,
    pub armed: bool,
    pub slot: u8,
}
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Scanning {
    pub dx: f64,
    pub dz: f64,
    pub until: u64,
}

pub static MODULE: Module = Module {
    name: "random_look",
    flags: MOVE | LOOK,
    blocked: Some(blocked),
    equip,
    arm,
    fired,
    can_use,
    start,
    stop,
    systems,
    requires: &[Cap::Body, Cap::Head],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let q = params::<Params>(p);
    w.entity_mut(e).insert(Scan { period: q.period, interval: q.interval, armed: false, slot: inst.slot });
}
fn systems(s: &mut Schedule, slot: GoalSlot) {
    s.add_systems(scan_run.in_set(slot));
}
fn arm(w: &mut World, e: Entity, inst: Inst) {
    let s = *w.get::<Scan>(e).unwrap();
    let p = w.get::<Pos>(e).unwrap().0;
    if !s.armed && (w.resource::<LazyLook>().watched(p.x, p.z) || holds_a_mover(w, e, inst)) {
        let wait = with_rng(w, e, |r| match s.interval {
            Some((lo, hi)) => lo + r.next_int(hi - lo + 1),
            None => 2 * geometric(r, s.period),
        });
        crate::goals::arm(w, e, inst.slot, wait);
        w.get_mut::<Scan>(e).unwrap().armed = true;
    }
}
/// The goal holds the MOVE flag against a goal of a larger priority number of the same animal (the panda's look 10 over its stroll 14): it
/// stops that goal and keeps it from starting whether or not a player sees the animal, so it is drawn and runs for nobody too (the
/// orientation alone is left out then: `scan_run`). Species whose look comes after every walk goal never need it.
fn holds_a_mover(w: &World, e: Entity, inst: Inst) -> bool {
    w.get::<GoalSet>(e).is_some_and(|g| {
        let own = g.entries()[inst.slot as usize].priority;
        g.entries().iter().any(|o| o.module.flags & MOVE != 0 && o.priority > own)
    })
}
fn rearm(w: &mut World, e: Entity, inst: Inst) {
    w.get_mut::<Scan>(e).unwrap().armed = false;
    arm(w, e, inst);
}
fn blocked(w: &mut World, e: Entity, inst: Inst) {
    rearm(w, e, inst);
}
fn fired(w: &mut World, e: Entity, _: Inst) -> bool {
    let Some(mut s) = w.get_mut::<Scan>(e) else { return false };
    s.armed = false;
    // With the lazy look, a timer that comes up for an animal no player sees lapses.
    let p = w.get::<Pos>(e).unwrap().0;
    w.resource::<LazyLook>().watched(p.x, p.z) || holds_a_mover(w, e, Inst { slot: s_slot(w, e), nth: 0 })
}
fn s_slot(w: &World, e: Entity) -> u8 {
    w.get::<Scan>(e).map_or(0, |s| s.slot)
}
fn can_use(_: &mut World, _: Entity, _: Inst) -> bool {
    true
}
fn start(w: &mut World, e: Entity, _: Inst) {
    let now = crate::cow::now_of(w, e);
    let (a, n) = with_rng(w, e, |r| (std::f64::consts::TAU * r.next_f64(), r.next_int(20)));
    let (dx, dz) = scan_dir(a);
    w.entity_mut(e).insert(Scanning { dx, dz, until: scan_until(now, n) });
}
fn stop(w: &mut World, e: Entity, inst: Inst) {
    w.entity_mut(e).remove::<Scanning>();
    rearm(w, e, inst);
}
/// `RandomLookAroundGoal.tick`.
fn scan_run(clock: Res<Clock>, lazy: Res<LazyLook>, mut q: Query<(Aim, &Scanning, &Scan)>, mut reqs: ResMut<Requests>, mut cmd: Commands) {
    for ((e, pos, mut look, looking, _, _), s, data) in &mut q {
        if clock.of(e) >= s.until {
            reqs.0.push(Req::End(e, data.slot));
        } else if lazy.watched(pos.0.x, pos.0.z) {
            let at = [pos.0.x + s.dx, pos.0.y + look.eye, pos.0.z + s.dz];
            aim(&mut cmd, e, &mut look, looking, at, None);
        }
    }
}
