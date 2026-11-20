//! `WalkAway` of the piglin (`mobs/piglin.rs:1048-1139`, `SetWalkTargetAwayFrom.entity(AVOID_TARGET, speed, distance, ...)`):
//! while a point to keep away from is remembered (`Avoid`, written by whoever sees the threat), the animal walks to a spot
//! `distance` blocks from it, ten tries at most, the path ending at once when none is found. Woken by the `Avoid` write.
use crate::goals;
use super::*;
use crate::{cow::*, goals::*};
use horizon_core::geometry::Vec3;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub speed: f64,
    /// The range of the pick (`SetWalkTargetAwayFrom`: `getPosAway(mob, 16, 7, pos)`, the jar's 16).
    pub distance: f64,
    /// Starts only closer than this to the point (`SetWalkTargetAwayFrom.pos(.., 8, ..)`: `distanceToSqr(pos) >= 8^2` refuses).
    pub within: f64,
}
impl Params {
    pub const DEFAULT: Self = Self { speed: 1.0, distance: 16.0, within: 8.0 };
}
/// The point to keep away from, and the time its memory expires (its ttl).
#[derive(Component, Clone, Copy, Debug)]
pub struct Avoid {
    pub at: [f64; 3],
    pub until: u64,
}
#[derive(Component, Clone, Copy, Debug)]
pub struct Away(pub Params);
/// The goal runs: the walk away is under way.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Walking;

pub static MODULE: Module = Module {
    name: "walk_away",
    flags: MOVE,
    ends_with_path: true,
    equip,
    can_use,
    start,
    stop,
    fired,
    requires: &[Cap::Body, Cap::Goals, Cap::Locomotion],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, _: Inst, p: &(dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Away(*params::<Params>(p)));
}
/// `canUse`: a remembered point that has not expired, and closer than `within` (the animal that has walked clear stays).
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    let (Some(a), Some(p), Some(away)) = (w.get::<Avoid>(e), w.get::<Pos>(e), w.get::<Away>(e)) else { return false };
    let d2 = (p.0.x - a.at[0]).powi(2) + (p.0.y - a.at[1]).powi(2) + (p.0.z - a.at[2]).powi(2);
    now_of(w, e) < a.until && d2 < away.0.within * away.0.within
}
/// The spot `distance` blocks away from the point (`land_random_pos_away`, five blocks down and seven up at most); none
/// found: the goal ends at once.
fn start(w: &mut World, e: Entity, inst: Inst) {
    let p = w.get::<Away>(e).unwrap().0;
    let a = w.get::<Avoid>(e).copied().unwrap();
    let from = Vec3::new(a.at[0], a.at[1], a.at[2]);
    // Ten tries, the first spot found is taken (`SetWalkTargetAwayFrom`: `for (i < 10) getPosAway(mob, 16, 7, pos)`).
    let r = (0..10).find_map(|_| goals::land_random_pos_away(w, e, from, p.distance, 7));
    match r {
        Some(at) => {
            go_to_at(w, e, at, p.speed, 0);
            w.entity_mut(e).insert(Walking);
        }
        None => w.resource_mut::<Requests>().0.push(Req::End(e, inst.slot)),
    }
}
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<Walking>();
    stop_nav(w, e);
}
/// A deadline that is not the goal's own (the replay of a start) asks `canUse`; a running goal has none.
fn fired(w: &mut World, e: Entity, _: Inst) -> bool {
    w.get::<Walking>(e).is_none()
}
