//! The smallest module (compiled with the tests; see LISEZMOI.md): a goal with no flag that, every `every` ticks, starts
//! and ends at once and counts itself. Copy this file to start a behaviour.
use super::*;
use crate::goals::*;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub every: i32,
}
impl Params {
    pub const DEFAULT: Self = Self { every: 100 };
}
#[derive(Component, Clone, Copy, Debug)]
pub struct Sniff {
    pub every: i32,
    pub done: u32,
}
pub static MODULE: Module = Module {
    name: "example",
    flags: 0,
    equip,
    arm,
    can_use,
    start,
    stop,
    requires: &[Cap::Body],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, _: Inst, p: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Sniff { every: params::<Params>(p).every, done: 0 });
}
fn arm(w: &mut World, e: Entity, inst: Inst) {
    let every = w.get::<Sniff>(e).unwrap().every;
    crate::goals::arm(w, e, inst.slot, every);
}
fn can_use(_: &mut World, _: Entity, _: Inst) -> bool {
    true
}
fn start(w: &mut World, e: Entity, inst: Inst) {
    w.get_mut::<Sniff>(e).unwrap().done += 1;
    w.resource_mut::<Requests>().0.push(Req::End(e, inst.slot));
}
fn stop(w: &mut World, e: Entity, inst: Inst) {
    arm(w, e, inst);
}

#[cfg(test)]
mod tests {
    use super::*;
    /// A species entry for the module resolves into a selector with its flags; the module is not in the table (a real
    /// one would be: one line in `TABLE`).
    #[test]
    fn a_module_is_one_entry_of_a_configuration() {
        static ENTRIES: [Entry; 1] = [Entry::new(&MODULE, 5, &Params { every: 40 })];
        let g = GoalSet::new(&ENTRIES);
        assert_eq!(g.slot_of(&MODULE), Some(0));
        assert!(g.replaceable(0) && !g.is_running(&MODULE));
    }
}
