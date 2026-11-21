//! The villager's two strolls toward its job sites, kids of the work `RunOne` (`StrollToPoi`, `StrollToPoiList`, `villager_brain.rs:2140-2175`;
//! the jar's `StrollToPoi.create` and `StrollToPoiList.create`, one-shot): `can_start` decides and acts, `start` has nothing left to do.
//! `StrollToPoi`: within 10 blocks of the job site, walk to it (speed 0.4, close 1) at most every 80 ticks. `StrollToPoiList`: pick one of the
//! secondary sites (a draw each try), within 6 blocks of the job site, walk to it (speed 0.5, close 1) at most every 100 ticks.
//! Both need the walk target absent (`absent(WALK_TARGET)` of the jar's builders). A try that is not due yet counts as started (the `RunOne` takes it and does nothing), as in the `Mob` and the jar.
use super::block_reach::{centre_dist_sq, walk_to};
use crate::brain::sensors::secondary_job_site::SECONDARY_JOB_SITE;
use crate::brain::*;
use crate::cow::{now_of, Pos};
use crate::goals::with_rng;
use crate::rules::poi_strolls::{next_try, too_far};
use crate::species::villager::JOB_SITE;
use bevy_ecs::prelude::*;

#[derive(Component, Default, Clone, Copy)]
pub struct NextToPoi(pub u64);
#[derive(Component, Default, Clone, Copy)]
pub struct NextToPoiList(pub u64);

pub static TO_POI: Behaviour = Behaviour { name: "minecraft:villager_stroll_to_poi", mems: &[&JOB_SITE, &WALK_TARGET], one_shot: true, can_start: to_poi, start: nothing, ..Behaviour::DEFAULT };
pub static TO_POI_LIST: Behaviour = Behaviour { name: "minecraft:villager_stroll_to_poi_list", mems: &[&SECONDARY_JOB_SITE, &JOB_SITE, &WALK_TARGET], one_shot: true, can_start: to_list, start: nothing, ..Behaviour::DEFAULT };

fn nothing(_: &mut World, _: Entity, _: &Ctx) {}

fn site(w: &World, e: Entity) -> Option<[i32; 3]> {
    match get(w, e, &JOB_SITE) {
        Some(MemValue::Block(p)) => Some(*p),
        _ => None,
    }
}

fn to_poi(w: &mut World, e: Entity, _: &Ctx) -> bool {
    if present(w, e, &WALK_TARGET) {
        return false;
    }
    let Some(p) = site(w, e) else { return false };
    let Some(at) = w.get::<Pos>(e).map(|q| q.0) else { return false };
    if too_far(centre_dist_sq(p, at), 100.0) {
        return false;
    }
    let now = now_of(w, e);
    if now > w.get::<NextToPoi>(e).map_or(0, |n| n.0) {
        walk_to(w, e, p, 0.4, 1);
        w.entity_mut(e).insert(NextToPoi(next_try(now, 80)));
    }
    true
}

fn to_list(w: &mut World, e: Entity, _: &Ctx) -> bool {
    if present(w, e, &WALK_TARGET) {
        return false;
    }
    let (Some(p), Some(MemValue::Blocks(list))) = (site(w, e), get(w, e, &SECONDARY_JOB_SITE).cloned()) else { return false };
    if list.is_empty() {
        return false;
    }
    let target = list[with_rng(w, e, |r| r.next_int(list.len() as i32)) as usize];
    let Some(at) = w.get::<Pos>(e).map(|q| q.0) else { return false };
    if too_far(centre_dist_sq(p, at), 36.0) {
        return false;
    }
    let now = now_of(w, e);
    if now > w.get::<NextToPoiList>(e).map_or(0, |n| n.0) {
        walk_to(w, e, target, 0.5, 1);
        w.entity_mut(e).insert(NextToPoiList(next_try(now, 100)));
    }
    true
}
