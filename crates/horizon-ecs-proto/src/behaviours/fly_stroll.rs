//! `AbstractFish.FlyStrollGoal` (`RandomSwimmingGoal`): now and then (one chance in `interval` per ask) swim to a random spot of the water:
//! `DefaultRandomPos(10, 7)` drawn again, up to 10 times, until the block of the spot can be swum through. The wait, the arming and
//! the walk to the spot are `stroll`'s (the same state), only the pick differs. A school (`FollowFlockLeader`) is not ported.
use super::*;
use crate::{cow::*, goals::*};
use crate::rules::fly_stroll::view_vector;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// The speed modifier of the swim.
    pub speed: f64,
    /// One chance in `interval` ticks (`reducedTickDelay`) per ask: 120, `reducedTickDelay(120)`, as the wander.
    pub interval: i32,
    /// The direction the walk leans to instead of the view (`BeeWanderGoal`: towards a far hive): `(xd, zd)` or `None` for the view.
    pub toward: Option<fn(&World, Entity) -> Option<(f64, f64)>>,
}
impl Params {
    pub const DEFAULT: Self = Self { speed: 1.0, interval: 120, toward: None };
}
#[derive(Component, Clone, Copy, Debug)]
pub struct FlyStrollGoal {
    pub speed: f64,
    pub toward: Option<fn(&World, Entity) -> Option<(f64, f64)>>,
}

pub static MODULE: Module = Module {
    name: "fly_stroll",
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
fn equip(w: &mut World, e: Entity, _: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let p = params::<Params>(p);
    w.entity_mut(e).insert(FlyStrollGoal { speed: p.speed, toward: p.toward });
    if let Some(mut s) = w.get_mut::<Stroll>(e) {
        s.interval = p.interval;
    }
}
fn arm_now(w: &mut World, e: Entity) {
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
    let now = crate::cow::now_of(w, e);
    let active = w.get::<ActiveUntil>(e).map_or(0, |a| a.0);
    let Some(mut s) = w.get_mut::<Stroll>(e) else { return false };
    s.scheduled = false;
    !(s.walking || now >= active)
}
fn can_use(w: &mut World, e: Entity, inst: Inst) -> bool {
    use crate::spots::{spotter, Pick};
    let Some(pos) = w.get::<Pos>(e).map(|p| p.0) else { return false };
    let level = *w.resource::<Level>();
    let (Some(mut s), Some(rng), Some(nav)) = (w.get::<Stroll>(e).copied(), w.get::<Rng>(e).map(|r| r.0), w.get::<NavParams>(e).copied()) else { return false };
    let walk = w.get::<crate::spots::WalkValue>(e).copied().unwrap_or(crate::spots::WalkValue::Zero);
    let mut r = Rng(rng);
    // The tests of the comparison impose the spot of the `Mob` (its height is the entity's `Stroll.ty`, which they set).
    let forced = take_xz(w, e, inst.slot);
    let picked = if let Some((x, z)) = forced {
        Some([x, f64::from(s.ty), z])
    } else {
        // The view of the head (`Mob.view_vector`): the heading and the pitch.
        let (yaw, pitch) = (w.get::<Yaw>(e).map_or(0.0, |y| y.0), w.get::<crate::goals::Head>(e).map_or(0.0, |h| h.pitch));
        let view = view_vector(yaw, pitch);
        let (xd, zd) = w.get::<FlyStrollGoal>(e).and_then(|g| g.toward).and_then(|f| f(w, e)).unwrap_or(view);
        let mut sp = spotter(&level, [pos.x, pos.y, pos.z], &nav, walk, &mut r);
        sp.pick(Pick::Flying { xd, zd })
    };
    w.get_mut::<Rng>(e).unwrap().0 = r.0;
    let ok = match picked {
        Some(p) => {
            s.walking = true;
            s.tx = p[0];
            s.tz = p[2];
            s.ty = p[1].floor() as i32;
            true
        }
        None => false,
    };
    *w.get_mut::<Stroll>(e).unwrap() = s;
    if !ok {
        arm_now(w, e);
    }
    ok
}
fn start(w: &mut World, e: Entity, _: Inst) {
    let speed = w.get::<FlyStrollGoal>(e).map_or(1.0, |g| g.speed);
    w.get_mut::<Nav>(e).unwrap().speed = speed;
    w.entity_mut(e).insert(NeedsPath);
}
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.get_mut::<Stroll>(e).unwrap().walking = false;
    stop_nav(w, e);
    arm_now(w, e);
}
