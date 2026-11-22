//! `StrollToPoi(HOME, speed, close, max)` (`PiglinBruteAi`, idle): with no walk target, a remembered block within `max` blocks of the
//! entity becomes its walk target, at most once per 80 ticks. The cooldown is a memory that expires, not a counter.
use crate::brain::*;
use crate::species::villager::HOME;

#[derive(Clone, Copy)]
pub struct Params {
    pub speed: f32,
    pub close: i32,
    pub max: f64,
}

pub static BEHAVIOUR: Behaviour = Behaviour { name: "minecraft:stroll_to_poi", mems: &[&HOME, &WALK_TARGET, &STROLL_TO_POI_COOLDOWN], one_shot: true, can_start, ..Behaviour::DEFAULT };

fn can_start(w: &mut World, e: Entity, c: &Ctx) -> bool {
    let p = c.get::<Params>();
    if present(w, e, &WALK_TARGET) || present(w, e, &STROLL_TO_POI_COOLDOWN) {
        return false;
    }
    let Some(&MemValue::Block(b)) = get(w, e, &HOME) else { return false };
    let Some(at) = w.get::<Pos>(e).map(|q| q.0) else { return false };
    let (dx, dy, dz) = (f64::from(b[0]) + 0.5 - at.x, f64::from(b[1]) + 0.5 - at.y, f64::from(b[2]) + 0.5 - at.z);
    if dx * dx + dy * dy + dz * dz >= p.max * p.max {
        return false;
    }
    set(w, e, &STROLL_TO_POI_COOLDOWN, MemValue::Bool(true), Some(80));
    let point = [f64::from(b[0]) + 0.5, f64::from(b[1]), f64::from(b[2]) + 0.5];
    set(w, e, &WALK_TARGET, MemValue::Walk(WalkTarget { target: Tracker::Point(point), speed: p.speed, close: p.close }), None);
    true
}
