//! Marche jusqu'au bloc d'une mémoire (`WalkTarget` de `walk_to_block_memory`, `villager_brain.rs:3629`) et s'arrête à `range`
//! horizontal du centre. Sert au poste de travail, à la cloche, au lit.
use crate::brain::*;
use bevy_ecs::prelude::*;
use crate::cow::Pos;

pub struct Params {
    pub memory: &'static MemKey,
    pub speed: f64,
    pub range: f64,
}

/// Le centre du bloc `p` au sol.
pub fn block_centre(p: [i32; 3]) -> [f64; 3] {
    [f64::from(p[0]) + 0.5, f64::from(p[1]), f64::from(p[2]) + 0.5]
}
fn block(w: &World, e: Entity, k: &MemKey) -> Option<[i32; 3]> {
    match get(w, e, k)? {
        MemValue::Block(p) => Some(*p),
        _ => None,
    }
}
fn arrived(w: &World, e: Entity, p: [i32; 3], range: f64) -> bool {
    w.get::<Pos>(e).is_some_and(|pos| {
        let (dx, dz) = (f64::from(p[0]) + 0.5 - pos.0.x, f64::from(p[2]) + 0.5 - pos.0.z);
        dx * dx + dz * dz <= range * range
    })
}
fn can_start(w: &mut World, e: Entity, c: &Ctx) -> bool {
    let p = c.get::<Params>();
    block(w, e, p.memory).is_some_and(|b| !arrived(w, e, b, p.range))
}
fn start(w: &mut World, e: Entity, c: &Ctx) {
    let p = c.get::<Params>();
    if let Some(b) = block(w, e, p.memory) {
        walk_to(w, e, block_centre(b), p.speed);
    }
}
fn tick(w: &mut World, e: Entity, c: &Ctx) -> bool {
    let p = c.get::<Params>();
    let Some(b) = block(w, e, p.memory) else { return false };
    !arrived(w, e, b, p.range) && !nav_done(w, e)
}

pub static BEHAVIOUR: Behaviour = Behaviour { name: "reach_block", mems: &[], can_start, start, tick, ..Behaviour::DEFAULT };
