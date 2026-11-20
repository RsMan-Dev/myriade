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
pub struct Convert(pub Params);
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
    can_use,
    start,
    stop,
    tick: Some(tick),
    requires: &[Cap::Body, Cap::Goals],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, _: Inst, p: &(dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Convert(*params::<Params>(p)));
}
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    w.get::<Converting>(e).is_none() && goals::in_unsafe_place(w, e)
}
fn start(w: &mut World, e: Entity, _: Inst) {
    let after = w.get::<Convert>(e).unwrap().0.after;
    let until = now_of(w, e) + after;
    w.entity_mut(e).insert(Converting { until });
}
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<Converting>();
}
/// The animals whose count is due become the other species; a count whose place turned safe is dropped in `stop`.
fn tick(w: &mut World) {
    let due: Vec<Entity> = {
        let mut q = w.query::<(Entity, &Converting)>();
        q.iter(w).filter(|(e, c)| now_of(w, *e) >= c.until).map(|(e, _)| e).collect()
    };
    for e in due {
        let into = w.get::<Convert>(e).unwrap().0.into;
        w.entity_mut(e).remove::<Converting>();
        crate::conversion::convert(w, e, into, crate::conversion::ConvertKeep::ALL);
    }
}
