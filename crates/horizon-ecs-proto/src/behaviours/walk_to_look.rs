//! `SetWalkTargetFromLookTarget` (one kid of the idle's `RunOne`): the animal walks to what it glances at (`Glancing`, set by
//! `look_at_player`), at its speed, along its stroll path. The `RunOne` draws among its kids by weight; here the draw is a gate
//! of `weight` out of `total` each time the goal is asked. Needs the stroll of the animal (`Stroll`): the path is its target.
use super::*;
use crate::behaviours::look_at_player::Glancing;
use crate::targeting::Who;
use crate::{cow::*, goals::*};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// The walk speed of `SetWalkTargetFromLookTarget`.
    pub speed: f64,
    /// This kid's weight in the `RunOne` of the idle, out of `total` (the kids' weights added).
    pub weight: i32,
    pub total: i32,
}
impl Params {
    pub const DEFAULT: Self = Self { speed: 1.0, weight: 1, total: 3 };
}
/// The speed and the weight of the goal of this animal.
#[derive(Component, Clone, Copy, Debug)]
pub struct LookWalk(pub Params);

pub static MODULE: Module = Module {
    name: "walk_to_look",
    flags: MOVE,
    equip,
    can_use,
    start,
    stop,
    requires: &[Cap::Body],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, _: Inst, p: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(LookWalk(params::<Params>(p).clone()));
}
/// Walks to the glanced-at animal or player (its position now), when the draw of the weight is won.
fn can_use(w: &mut World, e: Entity, inst: Inst) -> bool {
    let Some(g) = w.get::<Glancing>(e).copied().filter(|g| g.slot == inst.slot) else { return false };
    let Some(p) = w.get::<LookWalk>(e).map(|l| l.0) else { return false };
    let at = match g.who {
        Who::Entity(c) => w.get::<Pos>(c).map(|q| (q.0.x, q.0.y, q.0.z)),
        Who::Player(id) => w.resource::<Online>().get(id).map(|q| (q.x, q.y, q.z)),
    };
    let Some((x, y, z)) = at else { return false };
    if !with_rng(w, e, |r| r.next_int(p.total) < p.weight) {
        return false;
    }
    let Some(mut s) = w.get::<Stroll>(e).copied() else { return false };
    (s.walking, s.tx, s.tz, s.ty) = (true, x, z, y.floor() as i32);
    *w.get_mut::<Stroll>(e).unwrap() = s;
    true
}
fn start(w: &mut World, e: Entity, _: Inst) {
    let speed = w.get::<LookWalk>(e).map_or(1.0, |l| l.0.speed);
    w.get_mut::<Nav>(e).unwrap().speed = speed;
    w.entity_mut(e).insert(NeedsPath);
}
fn stop(w: &mut World, e: Entity, _: Inst) {
    if let Some(mut s) = w.get_mut::<Stroll>(e) {
        s.walking = false;
    }
    stop_nav(w, e);
}
