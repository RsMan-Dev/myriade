//! `MoveThroughVillageGoal` (a zombie's, at night): within 6 sections of a village, walk to a village point of interest that someone holds (a
//! ticket taken) and that is not one of the last 16 visited, and on to the next; it ends within `width + distance` blocks of the point. Move flag.
//! The point is the nearest unvisited held one within 32 blocks (the `Mob` samples a land position in 15 x 7 whose neighbourhood holds one: not the
//! same law, DOUBT in `suivi/ecs-ecarts.md`). Not done: the way to a point behind a wooden door (the walk stops at the door), the doors of a creature that cannot break them.
//! Asked each 60 ticks (a deadline of its own), not every other tick.
use super::door_interact::closer_to_center;
use super::*;
use crate::hostile::Bright;
use crate::poi::PoiRegistry;
use crate::rules::move_through_village::{arm_wait, crossing_end_radius, is_close_to_village, remember_visited, section_of};
use crate::{cow::*, goals::*};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub speed: f64,
    pub only_at_night: bool,
    pub distance: i32,
}
impl Params {
    pub const ZOMBIE: Self = Self { speed: 1.0, only_at_night: true, distance: 4 };
}
#[derive(Component, Clone, Debug)]
pub struct Village {
    p: &'static Params,
    slot: u8,
}
/// The point chosen and the last visited (sparse: laid by the first `can_use` that finds a point; the animals that never
/// come near a village carry none).
#[derive(Component, Clone, Debug)]
#[component(storage = "SparseSet")]
pub struct Trip {
    poi: [i32; 3],
    visited: Vec<[i32; 3]>,
}
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Crossing;

pub static MODULE: Module = Module { name: "move_through_village", flags: MOVE, equip, arm, fired, can_use, start, stop, tick: Some(tick), requires: &[Cap::Locomotion], ..Module::DEFAULT };
fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Village { p: params::<Params>(p), slot: inst.slot });
}
fn arm(w: &mut World, e: Entity, inst: Inst) {
    let wait = with_rng(w, e, |r| arm_wait(|bound| r.next_int(bound)));
    crate::goals::arm(w, e, inst.slot, wait);
}
fn fired(w: &mut World, e: Entity, inst: Inst) -> bool {
    crate::goals::arm(w, e, inst.slot, crate::rules::move_through_village::PERIOD);
    // No village point anywhere: `can_use` could only say no (`sectionsToVillage` is 7), so the goal is not even asked.
    w.resource::<PoiRegistry>().village_sections().next().is_some()
}
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    let Some(v) = w.get::<Village>(e) else { return false };
    if v.p.only_at_night && w.get_resource::<Bright>().is_some_and(|d| d.0) {
        return false;
    }
    let Some(pos) = w.get::<Pos>(e).map(|p| p.0) else { return false };
    let here = [pos.x.floor() as i32, pos.y.floor() as i32, pos.z.floor() as i32];
    let reg = w.resource::<PoiRegistry>();
    // `ServerLevel.isCloseToVillage(pos, 6)`.
    if !is_close_to_village(reg.sections_to_village(section_of(here))) {
        return false;
    }
    let visited = w.get::<Trip>(e).map(|t| t.visited.clone()).unwrap_or_default();
    let Some(poi) = reg.occupied_in_range(here, 32).into_iter().find(|p| !visited.contains(p)) else { return false };
    match w.get_mut::<Trip>(e) {
        Some(mut t) => t.poi = poi,
        None => {
            w.entity_mut(e).insert(Trip { poi, visited });
        }
    }
    true
}
fn start(w: &mut World, e: Entity, _: Inst) {
    let (p, speed) = (w.get::<Trip>(e).unwrap().poi, w.get::<Village>(e).unwrap().p.speed);
    go_to_at(w, e, [f64::from(p[0]), f64::from(p[1]), f64::from(p[2])], speed, 0);
    w.entity_mut(e).insert(Crossing);
}
/// A point reached (or the walk ended) is visited; the list keeps its last 16.
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<Crossing>();
    let done = !w.entity(e).contains::<Following>();
    let pos = w.get::<Pos>(e).map(|p| p.0);
    let Some(d) = w.get::<Village>(e).map(|v| f64::from(v.p.distance)) else { return };
    if let (Some(pos), Some(mut t)) = (pos, w.get_mut::<Trip>(e)) {
        let poi = t.poi;
        if done || closer_to_center(poi, pos, d) {
            remember_visited(&mut t.visited, poi);
        }
    }
}
fn tick(w: &mut World) {
    crate::goals::tick_state(w, run);
}
fn run(q: Query<(Entity, &Village, &Trip, &Pos, &Bounds, Has<Following>, Has<NeedsPath>), With<Crossing>>, mut reqs: ResMut<Requests>) {
    for (e, v, t, pos, b, following, asked) in &q {
        // `canContinue`: the walk goes on and the point is farther than `width + distance`.
        if !(following || asked) || closer_to_center(t.poi, pos.0, crossing_end_radius(b.0.size().x, v.p.distance)) {
            reqs.0.push(Req::End(e, v.slot));
        }
    }
}
