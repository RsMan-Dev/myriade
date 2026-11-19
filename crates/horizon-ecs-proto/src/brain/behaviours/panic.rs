//! `AnimalPanic` (on land): started by a damage that makes it panic or while panicking, picks a spot 5 blocks out whenever the walk
//! is over, until its duration is up.
use crate::brain::*;
use crate::cow::*;
use crate::goals::stop_nav;

#[derive(Clone, Copy)]
pub struct Params {
    pub speed: f32,
}
pub static BEHAVIOUR: Behaviour = Behaviour { name: "minecraft:animal_panic", mems: &[&HURT_BY, &IS_PANICKING, &WALK_TARGET], can_start, start, tick, stop, ..Behaviour::DEFAULT };
fn can_start(w: &mut World, e: Entity, _: &Ctx) -> bool {
    matches!(get(w, e, &HURT_BY), Some(MemValue::Bool(true))) || present(w, e, &IS_PANICKING)
}
fn start(w: &mut World, e: Entity, _: &Ctx) {
    set(w, e, &IS_PANICKING, MemValue::Bool(true), None);
    erase(w, e, &WALK_TARGET);
    stop_nav(w, e);
}
fn tick(w: &mut World, e: Entity, c: &Ctx) -> bool {
    if nav_done(w, e)
        && let Some(p) = pick(w, e, crate::spots::Pick::Default { h: 5, v: 4 })
    {
        let speed = c.get::<Params>().speed;
        set(w, e, &WALK_TARGET, MemValue::Walk(WalkTarget { target: Tracker::Point(p), speed, close: 0 }), None);
    }
    true
}
fn stop(w: &mut World, e: Entity, _: &Ctx) {
    erase(w, e, &IS_PANICKING);
}
