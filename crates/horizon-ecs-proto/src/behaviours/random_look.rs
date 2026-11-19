//! `RandomLookAroundGoal`: now and then (one chance in `period` per ask) look in a random direction for 20 to 40 ticks.
use super::*;
use crate::{cow::*, goals::*};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub period: i32,
}
impl Params {
    pub const DEFAULT: Self = Self { period: 50 };
}
/// The ask period; `armed`: its deadline is in the wheel.
#[derive(Component, Clone, Copy, Debug)]
pub struct Scan {
    pub period: i32,
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
    requires: &[Cap::Body],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, inst: Inst, p: &(dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Scan { period: params::<Params>(p).period, armed: false, slot: inst.slot });
}
fn systems(s: &mut Schedule, slot: GoalSlot) {
    s.add_systems(scan_run.in_set(slot));
}
fn arm(w: &mut World, e: Entity, inst: Inst) {
    let s = *w.get::<Scan>(e).unwrap();
    let p = w.get::<Pos>(e).unwrap().0;
    if !s.armed && w.resource::<LazyLook>().watched(p.x, p.z) {
        let wait = with_rng(w, e, |r| 2 * geometric(r, s.period));
        crate::goals::arm(w, e, inst.slot, wait);
        w.get_mut::<Scan>(e).unwrap().armed = true;
    }
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
    w.resource::<LazyLook>().watched(p.x, p.z)
}
fn can_use(_: &mut World, _: Entity, _: Inst) -> bool {
    true
}
fn start(w: &mut World, e: Entity, _: Inst) {
    let now = crate::cow::now_of(w, e);
    let (a, n) = with_rng(w, e, |r| (std::f64::consts::TAU * r.next_f64(), r.next_int(20)));
    w.entity_mut(e).insert(Scanning { dx: a.cos(), dz: a.sin(), until: now + 20 + n as u64 + 1 });
}
fn stop(w: &mut World, e: Entity, inst: Inst) {
    w.entity_mut(e).remove::<Scanning>();
    rearm(w, e, inst);
}
/// `RandomLookAroundGoal.tick`.
fn scan_run(clock: Res<Clock>, mut q: Query<(Aim, &Scanning, &Scan)>, mut reqs: ResMut<Requests>, mut cmd: Commands) {
    for ((e, pos, mut look, looking, _, _), s, data) in &mut q {
        if clock.of(e) >= s.until {
            reqs.0.push(Req::End(e, data.slot));
        } else {
            let at = [pos.0.x + s.dx, pos.0.y + look.eye, pos.0.z + s.dz];
            aim(&mut cmd, e, &mut look, looking, at, None);
        }
    }
}
