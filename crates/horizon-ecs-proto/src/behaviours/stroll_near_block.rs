//! Flâne près d'un bloc mémorisé (`StrollAroundPoi`, `villager_brain.rs:1239`) : ne démarre que si l'on est à moins de `max_dist`
//! du bloc, puis marche vers un point tiré au hasard autour de soi (8 horizontal, 6 vertical : `land_random_pos(cx.w, 8, 6)`).
use crate::brain::*;
use bevy_ecs::prelude::*;
use crate::cow::Pos;
use crate::goals::*;

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
    w.get::<Pos>(e).is_some_and(|pos| {
        let (dx, dz) = (f64::from(p[0]) + 0.5 - pos.0.x, f64::from(p[2]) + 0.5 - pos.0.z);
        dx * dx + dz * dz <= d * d
    })
}
fn can_start(w: &mut World, e: Entity, c: &Ctx) -> bool {
    let p = c.get::<Params>();
    block(w, e, p.memory).is_some_and(|b| near(w, e, b, p.max_dist)) && nav_done(w, e)
}
fn start(w: &mut World, e: Entity, c: &Ctx) {
    let p = c.get::<Params>();
    let Some(at) = w.get::<Pos>(e).map(|pos| pos.0) else { return };
    let dx = with_rng(w, e, |r| r.next_int(2 * H + 1)) - H;
    let dy = with_rng(w, e, |r| r.next_int(2 * V + 1)) - V;
    let dz = with_rng(w, e, |r| r.next_int(2 * H + 1)) - H;
    walk_to(w, e, [at.x + f64::from(dx), at.y + f64::from(dy), at.z + f64::from(dz)], p.speed);
}
fn tick(w: &mut World, e: Entity, _: &Ctx) -> bool {
    !nav_done(w, e)
}

pub static BEHAVIOUR: Behaviour = Behaviour { name: "stroll_near_block", mems: &[], can_start, start, tick, ..Behaviour::DEFAULT };
