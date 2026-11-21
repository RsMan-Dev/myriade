//! `MoveThroughVillageGoal` (a zombie's, at night): within 6 sections of a village, walk to a village point of interest that someone holds (a
//! ticket taken) and that is not one of the last 16 visited, and on to the next; it ends within `width + distance` blocks of the point. Move flag.
//! The point is the nearest unvisited held one within 32 blocks (the `Mob` samples a land position in 15 x 7 whose neighbourhood holds one: not the
//! same law, DOUBT in the deviation log). Not done: the way to a point behind a wooden door (the walk stops at the door), the doors of a creature that cannot break them.
//! Asked each 60 ticks (a deadline of its own), not every other tick.
use super::door_interact::closer_to_center;
use super::*;
use crate::hostile::Daylight;
use crate::poi::PoiRegistry;
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
    p: Params,
    slot: u8,
    poi: [i32; 3],
    visited: Vec<[i32; 3]>,
}
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Crossing;

pub static MODULE: Module = Module { name: "move_through_village", flags: MOVE, equip, arm, fired, can_use, start, stop, tick: Some(tick), requires: &[Cap::Locomotion], ..Module::DEFAULT };
const PERIOD: i32 = 60;
fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Village { p: *params::<Params>(p), slot: inst.slot, poi: [0; 3], visited: Vec::new() });
}
fn arm(w: &mut World, e: Entity, inst: Inst) {
    let wait = with_rng(w, e, |r| 1 + r.next_int(PERIOD));
    crate::goals::arm(w, e, inst.slot, wait);
}
fn fired(w: &mut World, e: Entity, inst: Inst) -> bool {
    crate::goals::arm(w, e, inst.slot, PERIOD);
    // No village point anywhere: `can_use` could only say no (`sectionsToVillage` is 7), so the goal is not even asked.
    w.resource::<PoiRegistry>().village_sections().next().is_some()
}
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    let Some(v) = w.get::<Village>(e) else { return false };
    if v.p.only_at_night && w.get_resource::<Daylight>().is_some_and(|d| d.0) {
        return false;
    }
    let Some(pos) = w.get::<Pos>(e).map(|p| p.0) else { return false };
    let here = [pos.x.floor() as i32, pos.y.floor() as i32, pos.z.floor() as i32];
    let reg = w.resource::<PoiRegistry>();
    // `ServerLevel.isCloseToVillage(pos, 6)`.
    if reg.sections_to_village([here[0] >> 4, here[1] >> 4, here[2] >> 4]) > 6 {
        return false;
    }
    let visited = v.visited.clone();
    let Some(poi) = reg.occupied_in_range(here, 32).into_iter().find(|p| !visited.contains(p)) else { return false };
    w.get_mut::<Village>(e).unwrap().poi = poi;
    true
}
fn start(w: &mut World, e: Entity, _: Inst) {
    let v = w.get::<Village>(e).unwrap();
    let (p, speed) = (v.poi, v.p.speed);
    go_to_at(w, e, [f64::from(p[0]), f64::from(p[1]), f64::from(p[2])], speed, 0);
    w.entity_mut(e).insert(Crossing);
}
/// A point reached (or the walk ended) is visited; the list keeps its last 16.
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<Crossing>();
    let done = !w.entity(e).contains::<Following>();
    let pos = w.get::<Pos>(e).map(|p| p.0);
    if let (Some(pos), Some(mut v)) = (pos, w.get_mut::<Village>(e)) {
        let (poi, d) = (v.poi, f64::from(v.p.distance));
        if done || closer_to_center(poi, pos, d) {
            v.visited.push(poi);
            if v.visited.len() > 16 {
                v.visited.remove(0);
            }
        }
    }
}
fn tick(w: &mut World) {
    crate::goals::tick_state(w, run);
}
fn run(q: Query<(Entity, &Village, &Pos, &Bounds, Has<Following>, Has<NeedsPath>), With<Crossing>>, mut reqs: ResMut<Requests>) {
    for (e, v, pos, b, following, asked) in &q {
        // `canContinue`: the walk goes on and the point is farther than `width + distance`.
        if !(following || asked) || closer_to_center(v.poi, pos.0, b.0.size().x + f64::from(v.p.distance)) {
            reqs.0.push(Req::End(e, v.slot));
        }
    }
}
