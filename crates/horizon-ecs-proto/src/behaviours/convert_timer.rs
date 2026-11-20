//! Conversion of an animal kept in a place where it cannot live (the zombification of `PiglinAi`, `mobs/piglin.rs:1964-2000`):
//! while `goals::in_unsafe_place` holds, the animal counts `after` ticks of its own time and then becomes `into`
//! (`conversion::convert`, socle absent). The count is dropped when the place turns safe again (`stop`).
use crate::goals;
use super::*;
use crate::{cow::*, goals::*};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// The species it becomes, by name.
    pub into: &'static str,
    /// Ticks in the unsafe place before the conversion.
    pub after: u64,
}
impl Params {
    pub const DEFAULT: Self = Self { into: "", after: 300 };
}
#[derive(Component, Clone, Copy, Debug)]
pub struct Convert(pub &'static Params);
/// The count runs: the conversion is due at `until`.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Converting {
    pub until: u64,
}

pub static MODULE: Module = Module {
    name: "convert_timer",
    flags: 0,
    equip,
    arm,
    can_use,
    start,
    stop,
    requires: &[Cap::Body, Cap::Goals],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, _: Inst, p: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Convert(params::<Params>(p)));
}
/// One check at creation: the place a creature lives in does not change while it lives (the host's `UnsafePlace` names a dimension), so no
/// later poll exists; `stop` (place turned safe) is the only exit.
fn arm(w: &mut World, e: Entity, i: Inst) {
    goals::arm(w, e, i.slot, 1);
}
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    w.get::<Converting>(e).is_none() && goals::in_unsafe_place(w, e)
}
fn start(w: &mut World, e: Entity, _: Inst) {
    let after = w.get::<Convert>(e).unwrap().0.after;
    // The first check runs one tick after creation, the `Mob`'s count began with its first tick: one tick is already counted.
    let until = now_of(w, e) + after - 1;
    w.entity_mut(e).insert(Converting { until });
    crate::cow::after(w, e, (after - 1) as i32, count_over);
}
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<Converting>();
}
/// The deadline of a count: the conversion is asked at the selector's end of this tick, unless `stop` dropped the count.
fn count_over(w: &mut World, e: Entity) {
    if w.get::<Converting>(e).is_some_and(|c| now_of(w, e) >= c.until) {
        w.resource_mut::<Requests>().0.push(Req::Call(e, convert_now));
    }
}
fn convert_now(w: &mut World, e: Entity) {
    let (Some(c), Some(cv)) = (w.get::<Converting>(e).copied(), w.get::<Convert>(e).copied()) else { return };
    if now_of(w, e) >= c.until {
        w.entity_mut(e).remove::<Converting>();
        crate::conversion::convert(w, e, cv.0.into, crate::conversion::ConvertKeep::ALL);
    }
}
