//! The fly stroll (`HappyGhastAi` idle `RunOne`, the legacy `FlyStrollGoal`'s pick): with no walk target, a spot in the air along the head's
//! view (`Pick::Flying`) becomes the walk target.
use crate::brain::*;
use crate::cow::Yaw;
use crate::rules::fly_stroll::view_vector;

#[derive(Clone, Copy)]
pub struct Params {
    pub speed: f32,
}

pub static BEHAVIOUR: Behaviour = Behaviour { name: "minecraft:fly_stroll", mems: &[&WALK_TARGET], one_shot: true, can_start, ..Behaviour::DEFAULT };

fn can_start(w: &mut World, e: Entity, c: &Ctx) -> bool {
    if present(w, e, &WALK_TARGET) {
        return false;
    }
    let (yaw, pitch) = (w.get::<Yaw>(e).map_or(0.0, |y| y.0), w.get::<crate::goals::Head>(e).map_or(0.0, |h| h.pitch));
    let (xd, zd) = view_vector(yaw, pitch);
    match pick(w, e, crate::spots::Pick::Flying { xd, zd }) {
        Some(to) => set(w, e, &WALK_TARGET, MemValue::Walk(WalkTarget { target: Tracker::Point(to), speed: c.get::<Params>().speed, close: 0 }), None),
        None => erase(w, e, &WALK_TARGET),
    }
    true
}
