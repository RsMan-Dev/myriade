//! Flâne près d'un bloc mémorisé (`StrollAroundPoi`, `villager_brain.rs:1239`) : ne démarre que si l'on est à moins de `max_dist`
//! du bloc, puis marche vers un point tiré au hasard autour de soi (8 horizontal, 6 vertical : `land_random_pos(cx.w, 8, 6)`).
use crate::brain::*;
use bevy_ecs::prelude::*;
use crate::cow::Pos;
use crate::goals::*;
use crate::rules::stroll_near_block as rules;

pub struct Params {
    pub memory: &'static MemKey,
    pub speed: f64,
    pub max_dist: f64,
}

const H: i32 = 8;
const V: i32 = 6;

fn block(w: &World, e: Entity, k: &MemKey) -> Option<[i32; 3]> {
    match get(w, e, k)? {
        MemValue::Block(p) => Some(*p),
        _ => None,
    }
}
/// Distance horizontale au centre du bloc (approximation : `closer_to_center` n'est pas porté tel quel).
fn near(w: &World, e: Entity, p: [i32; 3], d: f64) -> bool {
    w.get::<Pos>(e).is_some_and(|pos| rules::near_centre(p, pos.0, d))
}
/// The next allowed stroll (tick), per memory (`StrollAroundPoi.nextOkStartTime`: one per behaviour instance, here per site memory).
#[derive(Component, Default, Clone, Copy)]
#[component(storage = "SparseSet")]
pub struct NextStroll(pub [(usize, u64); 2]);

fn slot(p: &Params) -> usize {
    p.memory as *const MemKey as usize
}
/// `StrollAroundPoi` (one-shot, `villager_brain.rs:2116`): near the site, a try not due yet counts as started and does nothing;
/// a due try draws a point around the villager and walks to it, then waits 180 ticks.
fn can_start(w: &mut World, e: Entity, c: &Ctx) -> bool {
    let p = c.get::<Params>();
    if !block(w, e, p.memory).is_some_and(|b| near(w, e, b, p.max_dist)) {
        return false;
    }
    let now = crate::cow::now_of(w, e);
    let mut n = w.get::<NextStroll>(e).copied().unwrap_or_default();
    let k = slot(p);
    let i = n.0.iter().position(|s| s.0 == k).or_else(|| n.0.iter().position(|s| s.0 == 0)).unwrap_or(0);
    if now <= n.0[i].1 && n.0[i].0 == k {
        return true;
    }
    // `StrollAroundPoi`: the land pick (8 sideways, 6 up or down) becomes the WALK_TARGET (close 1), or the target is erased when
    // there is none; the move-to-target sink walks it (and sees it change), no navigation is asked from here.
    match pick(w, e, crate::spots::Pick::Land { h: H, v: V }) {
        Some(to) => set(w, e, &WALK_TARGET, MemValue::Walk(WalkTarget { target: Tracker::Point(to), speed: p.speed as f32, close: 1 }), None),
        None => erase(w, e, &WALK_TARGET),
    }
    n.0[i] = (k, now + 180);
    w.entity_mut(e).insert(n);
    true
}
fn nothing(_: &mut World, _: Entity, _: &Ctx) {}

pub static BEHAVIOUR: Behaviour = Behaviour { name: "stroll_near_block", mems: &[&WALK_TARGET], one_shot: true, can_start, start: nothing, ..Behaviour::DEFAULT };
