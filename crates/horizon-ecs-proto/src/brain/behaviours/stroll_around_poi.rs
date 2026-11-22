//! `StrollAroundPoi(HOME, speed, max)` (`PiglinBruteAi`, idle): within `max` blocks of the remembered block, with no walk target, a spot
//! 8 blocks sideways and 6 up or down (the land pick) becomes the walk target.
use crate::brain::*;
use crate::rules::stroll_near_block::near_centre;
use crate::species::villager::HOME;

#[derive(Clone, Copy)]
pub struct Params {
    pub speed: f32,
    pub max: f64,
}

pub static BEHAVIOUR: Behaviour = Behaviour { name: "minecraft:stroll_around_poi", mems: &[&HOME, &WALK_TARGET], one_shot: true, can_start, ..Behaviour::DEFAULT };

fn can_start(w: &mut World, e: Entity, c: &Ctx) -> bool {
    let p = c.get::<Params>();
    if present(w, e, &WALK_TARGET) {
        return false;
    }
    let Some(&MemValue::Block(b)) = get(w, e, &HOME) else { return false };
    let Some(at) = w.get::<Pos>(e).map(|q| q.0) else { return false };
    if !near_centre(b, at, p.max) {
        return false;
    }
    match pick(w, e, crate::spots::Pick::Land { h: 8, v: 6 }) {
        Some(to) => set(w, e, &WALK_TARGET, MemValue::Walk(WalkTarget { target: Tracker::Point(to), speed: p.speed, close: 0 }), None),
        None => erase(w, e, &WALK_TARGET),
    }
    true
}
