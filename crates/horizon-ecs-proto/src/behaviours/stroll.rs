//! `RandomStrollGoal`: now and then (one chance in `interval` per ask, one ask every 2 ticks) walk to a random spot.
//! The attempt is a deadline of the wheel; the spot is the kernel's `pick_spot`.
use super::*;
use crate::rules::{spear_use::halved_use, stroll as rules};
use crate::{cow::*, goals::*};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// The speed modifier of the walk.
    pub speed: f64,
    /// The interval of the attempts in ticks (`RandomStrollGoal.interval`: 120; the turtle's 100), halved as `reducedTickDelay`.
    pub interval: Option<i32>,
    /// The chance of a plain spot (water allowed) instead of a land one: `WaterAvoidingRandomStrollGoal.probability`.
    pub water_avoiding: f32,
}
impl Params {
    pub const DEFAULT: Self = Self { speed: 1.0, interval: None, water_avoiding: 0.001 };
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
    damaged,
    requires: &[Cap::Locomotion],
    ..Module::DEFAULT
};
pub static MODULE_LOOK: Module = Module {
    name: "stroll_look",
    flags: MOVE | LOOK,
    ends_with_path: true,
    blocked: Some(blocked),
    equip,
    fired,
    can_use,
    start,
    stop,
    damaged,
    requires: &[Cap::Locomotion, Cap::Head],
    ..Module::DEFAULT
};
/// The module this entity's stroll entry uses: the guardian's `.looking()` stroll holds LOOK too, the same code otherwise.
pub fn module_of(w: &World, e: Entity) -> &'static Module {
    if w.get::<GoalSet>(e).is_some_and(|g| g.has(&MODULE_LOOK)) { &MODULE_LOOK } else { &MODULE }
}
/// `ai::after_hurt`: `noActionTime = 0`: the animal is active for 100 more ticks and its stroll attempt, if none is armed, is armed
/// (the wait is drawn from a stream of its own: the animal's random belongs to its goals, which the hit must not shift).
fn damaged(w: &mut World, e: Entity, _: Inst, _: &Damaged) {
    let now = crate::cow::now_of(w, e);
    let Some(uid) = w.get::<Uid>(e).map(|u| u.0) else { return };
    let Some(mut a) = w.get_mut::<ActiveUntil>(e) else { return };
    if a.0 == u64::MAX {
        return;
    }
    a.0 = a.0.max(now + 100);
    let Some(mut s) = w.get::<Stroll>(e).copied() else { return };
    if !s.scheduled && !s.walking {
        let mut own = Rng(rules::after_hurt_seed(uid, now));
        w.resource_scope(|w: &mut World, mut wheel: Mut<Wheel>| {
            let clock = w.resource::<Clock>();
            crate::cow::arm_stroll(&mut wheel, clock, e, &mut s, &mut own);
        });
        *w.get_mut::<Stroll>(e).unwrap() = s;
    }
}
fn equip(w: &mut World, e: Entity, _: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let p = params::<Params>(p);
    w.entity_mut(e).insert(StrollGoal { speed: p.speed });
    if let Some(mut s) = w.get_mut::<Stroll>(e) {
        if let Some(n) = p.interval {
            s.interval = halved_use(n, true);
        }
        s.plain = p.water_avoiding;
    }
}
/// `RandomStrollGoal.trigger`: the next ask of the goal picks a spot without the wait; a walk under way keeps the flag for its end.
pub fn trigger(w: &mut World, e: Entity) {
    let Some(mut s) = w.get_mut::<Stroll>(e) else { return };
    s.force = true;
    let walking = s.walking;
    drop(s);
    if !walking {
        wake(w, e, module_of(w, e));
    }
}
/// Arms the next attempt, unless one is armed.
pub fn arm_now(w: &mut World, e: Entity) {
    let mut s = *w.get::<Stroll>(e).unwrap();
    if s.scheduled {
        return;
    }
    let wait = with_rng(w, e, |r| idle_wait(r, s.interval));
    arm_module(w, e, module_of(w, e), wait);
    s.scheduled = true;
    *w.get_mut::<Stroll>(e).unwrap() = s;
}
fn blocked(w: &mut World, e: Entity, inst: Inst) {
    // A forced attempt waits for its flags (`canUse` is not asked while another goal holds them), without the wait.
    if w.get::<Stroll>(e).is_some_and(|s| s.force) {
        pend(w, e, inst.slot);
    } else {
        arm_now(w, e);
    }
}
fn fired(w: &mut World, e: Entity, _: Inst) -> bool {
    let now = crate::cow::now_of(w, e);
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
    let forced = take_xz(w, e, inst.slot);
    let ok = match forced {
        Some((x, z)) => {
            (s.walking, s.tx, s.tz) = (true, x, z);
            true
        }
        None => {
            let nav = w.get::<NavParams>(e).copied();
            let walk = w.get::<crate::spots::WalkValue>(e).copied().unwrap_or(crate::spots::WalkValue::Zero);
            let in_water = w.get::<FluidState>(e).is_some_and(|f| f.in_water);
            match nav {
                Some(nav) => pick_spot(&level, &Pos(pos), &mut s, &mut r, &nav, walk, in_water),
                None => false,
            }
        }
    };
    if ok {
        s.force = false;
    }
    w.get_mut::<Rng>(e).unwrap().0 = r.0;
    *w.get_mut::<Stroll>(e).unwrap() = s;
    if !ok {
        if s.force {
            wake(w, e, module_of(w, e));
        } else {
            arm_now(w, e);
        }
    }
    ok
}
fn start(w: &mut World, e: Entity, _: Inst) {
    let speed = w.get::<StrollGoal>(e).map_or(1.0, |g| g.speed);
    w.get_mut::<Nav>(e).unwrap().speed = speed;
    crate::pathsearch::probe!(ASKS[0].fetch_add(1, std::sync::atomic::Ordering::Relaxed));
    w.entity_mut(e).insert(NeedsPath);
}
fn stop(w: &mut World, e: Entity, _: Inst) {
    let s = w.get_mut::<Stroll>(e).map(|mut s| {
        s.walking = false;
        s.force
    });
    stop_nav(w, e);
    if s == Some(true) {
        wake(w, e, module_of(w, e));
    } else {
        arm_now(w, e);
    }
}
