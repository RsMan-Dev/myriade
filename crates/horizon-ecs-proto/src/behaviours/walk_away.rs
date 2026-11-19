//! `WalkAway` of the piglin (`mobs/piglin.rs:1048-1139`, `SetWalkTargetAwayFrom.entity(AVOID_TARGET, speed, distance, ...)`):
//! while a point to keep away from is remembered (`Avoid`, written by whoever sees the threat), the animal walks to a spot
//! `distance` blocks from it, ten tries at most, the path ending at once when none is found. Woken by the `Avoid` write.
use super::*;
use crate::{cow::*, goals::*};
use horizon_core::geometry::Vec3;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub speed: f64,
    /// Blocks from the point the animal walks to.
    pub distance: f64,
}
impl Params {
    pub const DEFAULT: Self = Self { speed: 1.0, distance: 12.0 };
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
/// `canUse`: a remembered point that has not expired.
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    w.get::<Avoid>(e).is_some_and(|a| now_of(w, e) < a.until)
}
/// The spot `distance` blocks away from the point (`land_random_pos_away`, five blocks down and seven up at most); none
/// found: the goal ends at once.
fn start(w: &mut World, e: Entity, inst: Inst) {
    let p = w.get::<Away>(e).unwrap().0;
    let a = w.get::<Avoid>(e).copied().unwrap();
    let from = Vec3::new(a.at[0], a.at[1], a.at[2]);
    match goals::land_random_pos_away(w, e, from, p.distance, 7) {
        Some(at) => {
            go_to_at(w, e, at, p.speed, 1);
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
