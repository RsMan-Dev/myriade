//! `RandomStroll.stroll`: a walk target within `h` x `v` blocks (none if no spot is standable), unless there is one.
use crate::brain::*;
use crate::cow::*;

#[derive(Clone, Copy)]
pub struct Params {
    pub speed: f32,
    pub h: i32,
    pub v: i32,
    /// Only out of water (the axolotl's land stroll: `Stroll` refuses while it swims).
    pub dry_only: bool,
}
pub static BEHAVIOUR: Behaviour = Behaviour { name: "minecraft:random_stroll", mems: &[&WALK_TARGET], one_shot: true, can_start, ..Behaviour::DEFAULT };
fn can_start(w: &mut World, e: Entity, c: &Ctx) -> bool {
    let p = c.get::<Params>();
    if p.dry_only && w.get::<FluidState>(e).is_some_and(|f| f.in_water) {
        return false;
    }
    if present(w, e, &WALK_TARGET) {
        return false;
    }
    match pick(w, e, crate::spots::Pick::Land { h: p.h, v: p.v }) {
        Some(at) => set(w, e, &WALK_TARGET, MemValue::Walk(WalkTarget { target: Tracker::Point(at), speed: p.speed, close: 0 }), None),
        None => erase(w, e, &WALK_TARGET),
    }
    true
}
