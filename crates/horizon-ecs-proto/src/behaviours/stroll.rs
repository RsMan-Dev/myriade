//! `RandomStrollGoal`: now and then (one chance in `interval` per ask, one ask every 2 ticks) walk to a random spot.
//! The attempt is a deadline of the wheel; the spot is the kernel's `pick_spot`.
use super::*;
use crate::{cow::*, goals::*};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// The speed modifier of the walk.
    pub speed: f64,
}
impl Params {
    pub const DEFAULT: Self = Self { speed: 1.0 };
}
#[derive(Component, Clone, Copy, Debug)]
pub struct StrollGoal {
    pub speed: f64,
}
/// What `Due::Stroll` (the kernel's own stroll timer) fires: the animal's stroll goal, whatever its rank.
pub const FIRED: u8 = u8::MAX;

pub static MODULE: Module = Module {
    name: "stroll",
    flags: MOVE,
    ends_with_path: true,
    blocked: Some(blocked),
    equip,
    fired,
    can_use,
    start,
    stop,
    requires: &[Cap::Locomotion],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, _: Inst, p: &(dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(StrollGoal { speed: params::<Params>(p).speed });
}
/// Arms the next attempt, unless one is armed.
pub fn arm_now(w: &mut World, e: Entity) {
    let mut s = *w.get::<Stroll>(e).unwrap();
    if s.scheduled {
        return;
    }
    let wait = with_rng(w, e, |r| idle_wait(r, s.interval));
    arm_module(w, e, &MODULE, wait);
    s.scheduled = true;
    *w.get_mut::<Stroll>(e).unwrap() = s;
}
fn blocked(w: &mut World, e: Entity, _: Inst) {
    arm_now(w, e);
}
fn fired(w: &mut World, e: Entity, _: Inst) -> bool {
    let now = w.resource::<Clock>().0;
    let active = w.get::<ActiveUntil>(e).map_or(0, |a| a.0);
    let Some(mut s) = w.get_mut::<Stroll>(e) else { return false };
    s.scheduled = false;
    !(s.walking || now >= active)
}
fn can_use(w: &mut World, e: Entity, inst: Inst) -> bool {
    let Some(pos) = w.get::<Pos>(e).map(|p| p.0) else { return false };
    let level = *w.resource::<Level>();
    let (Some(mut s), Some(rng)) = (w.get::<Stroll>(e).copied(), w.get::<Rng>(e).map(|r| r.0)) else { return false };
    let mut r = Rng(rng);
    let forced = w.resource_mut::<Forced>().spots.remove(&(e, inst.slot));
    let ok = match forced {
        Some((x, z)) => {
            (s.walking, s.tx, s.tz) = (true, x, z);
            true
        }
        None => pick_spot(&level, &Pos(pos), &mut s, &mut r),
    };
    w.get_mut::<Rng>(e).unwrap().0 = r.0;
    *w.get_mut::<Stroll>(e).unwrap() = s;
    if !ok {
        arm_now(w, e);
    }
    ok
}
fn start(w: &mut World, e: Entity, _: Inst) {
    let speed = w.get::<StrollGoal>(e).map_or(1.0, |g| g.speed);
    w.get_mut::<Nav>(e).unwrap().speed = speed;
    ASKS[0].fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    w.entity_mut(e).insert(NeedsPath);
}
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.get_mut::<Stroll>(e).unwrap().walking = false;
    stop_nav(w, e);
    arm_now(w, e);
}
