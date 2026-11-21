//! `Raider.RaiderMoveThroughVillageGoal`: in an active raid, with no target, walk to a home (bed) of the village within 48 blocks that is not one of
//! the last three visited, then on to the next. It walks toward it in hops (16 blocks within a tenth of a half-turn, else 8 within a half-turn) and gives
//! up when no hop is found; it ends within `width + distance` blocks of it, or with a target, or when the raid is over. Move flag, asked again when it ends.
//! The homes are the registry's (`poi.rs`, within 48 blocks of the raider): the `Mob` scans the blocks around the raid's centre for beds.
use super::door_interact::closer_to_center;
use super::*;
use crate::poi::PoiRegistry;
use crate::rules::move_towards_restriction::home_target;
use crate::raids::has_active_raid;
use crate::spots::{pick_for, Pick};
use crate::targeting::Target;
use crate::{cow::*, goals::*};
use horizon_world::poi::PoiType;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub speed: f64,
    pub distance: f64,
}
impl Params {
    pub const DEFAULT: Self = Self { speed: 1.05, distance: 1.0 };
}
#[derive(Component, Clone, Debug)]
pub struct Through {
    p: Params,
    slot: u8,
    poi: [i32; 3],
    visited: Vec<[i32; 3]>,
}
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Visiting {
    stuck: bool,
}

pub static MODULE: Module = Module { name: "raid_move_through_village", flags: MOVE, retry_on_end: true, equip, can_use, start, stop, tick: Some(tick), requires: &[Cap::Locomotion], ..Module::DEFAULT };
fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Through { p: *params::<Params>(p), slot: inst.slot, poi: [0; 3], visited: Vec::new() });
}
fn block(p: horizon_core::geometry::Vec3) -> [i32; 3] {
    [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32]
}
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    if !has_active_raid(w, e) || w.get::<Target>(e).is_some() {
        return false;
    }
    let Some(pos) = w.get::<Pos>(e).map(|p| p.0) else { return false };
    let mut t = w.get_mut::<Through>(e).unwrap();
    if t.visited.len() > 2 {
        t.visited.remove(0);
    }
    let visited = t.visited.clone();
    let near: Vec<[i32; 3]> = w.resource::<PoiRegistry>().in_range(block(pos), 64, false, |k| k == PoiType::Home).into_iter().map(|(p, _)| p).filter(|p| !visited.contains(p) && closer_to_center(*p, pos, 48.0)).collect();
    if near.is_empty() {
        return false;
    }
    let i = with_rng(w, e, |r| r.next_int(near.len() as i32)) as usize;
    w.get_mut::<Through>(e).unwrap().poi = near[i];
    true
}
fn start(w: &mut World, e: Entity, _: Inst) {
    let t = w.get::<Through>(e).unwrap();
    let (p, speed) = (t.poi, t.p.speed);
    go_to_at(w, e, [f64::from(p[0]), f64::from(p[1]), f64::from(p[2])], speed, 1);
    w.entity_mut(e).insert(Visiting { stuck: false });
}
fn stop(w: &mut World, e: Entity, _: Inst) {
    let pos = w.get::<Pos>(e).map(|p| p.0);
    if let (Some(pos), Some(mut t)) = (pos, w.get_mut::<Through>(e)) {
        let (poi, d) = (t.poi, t.p.distance);
        if closer_to_center(poi, pos, d) {
            t.visited.push(poi);
        }
    }
    w.entity_mut(e).remove::<Visiting>();
}
fn tick(w: &mut World) {
    crate::goals::tick_state(w, run);
}
#[allow(clippy::type_complexity)]
fn run(q: Query<(Entity, &Through, &Visiting, Has<Following>, Has<NeedsPath>)>, mut cmd: Commands) {
    for (e, t, v, following, asked) in &q {
        let (slot, poi, p, stuck) = (t.slot, t.poi, t.p, v.stuck);
        cmd.queue(move |w: &mut World| {
            let Some(pos) = w.get::<Pos>(e).map(|p| p.0) else { return };
            let width = w.get::<Bounds>(e).map_or(0.6, |b| b.0.size().x);
            // `canContinue`: the path is not done, no target, not there yet, not stuck.
            if stuck || w.get::<Target>(e).is_some() || closer_to_center(poi, pos, width + p.distance) || !(following || asked) && !has_active_raid(w, e) {
                w.resource_mut::<Requests>().0.push(Req::End(e, slot));
                return;
            }
            if following || asked {
                return;
            }
            let to = home_target(poi);
            let next = pick_for(w, e, Pick::DefaultTowards { h: 16, v: 7, to, max_rad: std::f64::consts::PI / 10.0 }).or_else(|| pick_for(w, e, Pick::DefaultTowards { h: 8, v: 7, to, max_rad: std::f64::consts::FRAC_PI_2 }));
            match next {
                Some(at) => go_to_at(w, e, at, p.speed, 1),
                None => w.get_mut::<Visiting>(e).unwrap().stuck = true,
            }
        });
    }
}
