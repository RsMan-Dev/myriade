//! Marche jusqu'au bloc d'une mémoire (`WalkToBlockMemory`, `walk_to_block_memory`, `villager_brain.rs:3629`) et s'arrête à `range`
//! horizontal du centre. Sert au poste de travail, à la cloche, au lit.
use crate::brain::*;
use crate::rules::reach_block as rules;
pub use crate::rules::reach_block::block_centre;
use crate::cow::{now_of, Pos};
use crate::spots::Pick;
use bevy_ecs::prelude::*;

pub struct Params {
    pub memory: &'static MemKey,
    pub speed: f64,
    pub range: f64,
    /// Manhattan distance beyond which the walk is a random step towards the block (`too_far`).
    pub too_far: i32,
    /// Ticks a failed walk may last before the memory is dropped (`too_long`), counted from the failure's stamp.
    pub too_long: i64,
}

/// `cantReachWalkTargetSince`: the time the walk first ended short of its block; cleared on arrival.
pub static SINCE: MemKey = MemKey::new("horizon:reach_since", Shape::Int);
/// The random step being walked to (`WalkTarget.point`): its end is not a failure.
pub static STEP: MemKey = MemKey::new("horizon:reach_step", Shape::Block);

fn block(w: &World, e: Entity, k: &MemKey) -> Option<[i32; 3]> {
    match get(w, e, k)? {
        MemValue::Block(p) => Some(*p),
        _ => None,
    }
}
fn arrived(w: &World, e: Entity, p: [i32; 3], range: f64) -> bool {
    w.get::<Pos>(e).is_some_and(|pos| rules::arrived(p, pos.0, range))
}
fn here(w: &World, e: Entity) -> Option<[i32; 3]> {
    w.get::<Pos>(e).map(|q| [q.0.x.floor() as i32, q.0.y.floor() as i32, q.0.z.floor() as i32])
}
/// `PathNavigation.moveTo` with the reach of a post: the path may end beside a block with a body (a composter, a bell).
fn walk(w: &mut World, e: Entity, at: [f64; 3], speed: f64) {
    walk_to(w, e, at, speed);
    if let Some(mut n) = w.get_mut::<crate::cow::Nav>(e) {
        n.reach = 1;
    }
}
/// `walk_to_block_memory`: the walk to the block, or a random step towards it when it is too far (up to 1000 draws, then the
/// memory is dropped and the failure stamped).
fn can_start(w: &mut World, e: Entity, c: &Ctx) -> bool {
    let p = c.get::<Params>();
    let (Some(h), Some(b)) = (here(w, e), block(w, e, p.memory)) else { return false };
    if !nav_done(w, e) {
        return false;
    }
    if arrived(w, e, b, p.range) {
        erase(w, e, &SINCE);
        return false;
    }
    let now = now_of(w, e) as i64;
    if int(w, e, &SINCE).is_some_and(|s| now - s > p.too_long) {
        erase(w, e, p.memory);
        set(w, e, &SINCE, MemValue::Int(now), None);
        return false;
    }
    if rules::manhattan(b, h) <= p.too_far {
        erase(w, e, &STEP);
        walk(w, e, block_centre(b), p.speed);
        return true;
    }
    let to = block_centre(b);
    for _ in 0..1000 {
        let Some(s) = pick(w, e, Pick::DefaultTowards { h: 15, v: 7, to, max_rad: f64::from(std::f32::consts::FRAC_PI_2) }) else { continue };
        let q = [s[0].floor() as i32, s[1].floor() as i32, s[2].floor() as i32];
        if rules::manhattan(q, h) <= p.too_far {
            set(w, e, &STEP, MemValue::Block(q), None);
            walk(w, e, s, p.speed);
            return true;
        }
    }
    erase(w, e, p.memory);
    set(w, e, &SINCE, MemValue::Int(now), None);
    false
}
/// The walk ends: at the block it is done; short of it, the failure is stamped (once) unless the step was reached.
fn tick(w: &mut World, e: Entity, c: &Ctx) -> bool {
    let p = c.get::<Params>();
    let Some(b) = block(w, e, p.memory) else { return false };
    if arrived(w, e, b, p.range) {
        erase(w, e, &SINCE);
        return false;
    }
    if !nav_done(w, e) {
        return true;
    }
    let step_reached = block(w, e, &STEP).is_some_and(|s| w.get::<Pos>(e).is_some_and(|q| rules::step_reached(s, q.0)));
    if !step_reached && !present(w, e, &SINCE) {
        set(w, e, &SINCE, MemValue::Int(now_of(w, e) as i64), None);
    }
    false
}

pub static BEHAVIOUR: Behaviour = Behaviour { name: "reach_block", mems: &[&SINCE, &STEP], can_start, tick, ..Behaviour::DEFAULT };
