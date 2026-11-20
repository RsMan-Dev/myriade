//! `NearestLivingEntitySensor`: every living entity (a player among them) whose box meets the follow range, nearest first;
//! written to `mobs` (absent when empty) and `visible_mobs` (present even when empty; its sight cache starts again).
use crate::behaviours::Cap;
use crate::brain::*;
use crate::cow::*;
use crate::goals::{d2, Online};
use crate::targeting::{FollowRange, Who};
use horizon_core::geometry::Vec3;

pub static SENSOR: Sensor = Sensor { name: "minecraft:nearest_living_entities", mems: &[&MOBS, &VISIBLE_MOBS], scan, requires: &[Cap::Targets], ..Sensor::DEFAULT };

/// `Brain.living_near` before its sort: the candidates in visit order with their squared distances, into `out` (cleared first).
pub fn living_candidates(w: &World, e: Entity, out: &mut Vec<(Who, f64)>) {
    out.clear();
    let (Some(p), Some(r)) = (w.get::<Pos>(e).map(|p| p.0), w.get::<FollowRange>(e).map(|f| f.0)) else { return };
    let bounds = w.get::<Bounds>(e).map_or(crate::cow::box_at(p, 0.6, 1.0), |b| b.0);
    let area = bounds.inflate(Vec3::new(r, r, r));
    w.resource::<Index>().for_radius(p.x, p.z, r + 2.0, |o| {
        if o != e
            && w.get::<Health>(o).is_some()
            && let Some(b) = w.get::<Bounds>(o)
            && b.0.intersects(area)
        {
            out.push((Who::Entity(o), d2(p, w.get::<Pos>(o).unwrap().0)));
        }
    });
    w.resource::<Online>().for_near(p.x, p.z, r + 2.0, |pl| {
        let b = crate::targeting::player_box(&pl);
        if w.resource::<crate::hostile::PlayerState>().alive(pl.id) && b.intersects(area) {
            out.push((Who::Player(pl.id), d2(p, Vec3::new(pl.x, pl.y, pl.z))));
        }
    });
}
/// `Brain.living_near`.
pub fn living_near(w: &World, e: Entity) -> Vec<Who> {
    let mut found = Vec::new();
    living_candidates(w, e, &mut found);
    found.sort_by(|a, b| a.1.total_cmp(&b.1));
    found.into_iter().map(|x| x.0).collect()
}
fn scan(w: &mut World, e: Entity, _: &Ctx) {
    let list = living_near(w, e);
    set(w, e, &MOBS, MemValue::Whos(list.clone()), None);
    clear_sight(w, e);
    set(w, e, &VISIBLE_MOBS, MemValue::Whos(list), None);
}
