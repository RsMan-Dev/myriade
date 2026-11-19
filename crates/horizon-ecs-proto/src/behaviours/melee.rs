//! `MeleeAttackGoal`: chase the target the target goal picked and strike it. `canUse` runs on a 20-tick grid, armed in
//! the wheel; the goal ticks (look, path every 4 to 10 ticks, strike) only while it runs.
use super::*;
use crate::targeting::{self, Target};
use crate::{cow::*, goals::*, hostile};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// The speed modifier of the chase.
    pub speed: f64,
}
impl Params {
    pub const DEFAULT: Self = Self { speed: 1.0 };
}

pub static MODULE: Module = Module {
    name: "melee",
    flags: MOVE | LOOK,
    ends_with_path: true,
    resources,
    equip,
    fired,
    can_use,
    start,
    stop,
    systems,
    requires: &[Cap::Targets, Cap::Locomotion],
    ..Module::DEFAULT
};
/// The target service tells the melee goal when its animal gets a target: the first check is armed on the grid.
fn resources(w: &mut World) {
    w.resource_mut::<targeting::TargetWatchers>().0.push(|w, e| {
        if w.get::<hostile::Attack>(e).is_some_and(|a| a.melee_slot != u8::MAX) {
            hostile::arm_melee(w, e);
        }
    });
}
fn equip(w: &mut World, e: Entity, inst: Inst, p: &(dyn std::any::Any + Sync)) {
    hostile::ensure_attack(w, e);
    let mut a = w.get_mut::<hostile::Attack>(e).unwrap();
    a.melee_slot = inst.slot;
    a.speed = params::<Params>(p).speed;
}
fn systems(s: &mut Schedule, slot: GoalSlot) {
    s.add_systems(hostile::melee_run.in_set(slot));
}
/// `canUse` is asked only on the ticks where tick + id is even; the 20-tick wait counts from the last ask.
fn fired(w: &mut World, e: Entity, inst: Inst) -> bool {
    let now = w.resource::<Clock>().0;
    if now > 1 && (now + phase_of(w, e)) % 2 == 1 {
        arm(w, e, inst.slot, 1);
        return false;
    }
    if let Some(mut a) = w.get_mut::<hostile::Attack>(e) {
        a.armed = false;
        a.phase = now;
    }
    if w.get::<Target>(e).is_none() {
        return false;
    }
    // The check on the grid: start now; if it did not start, the next check is armed.
    start_now(w, e, inst);
    false
}
fn start_now(w: &mut World, e: Entity, inst: Inst) {
    crate::goals::try_start(w, e, inst.slot);
    if w.get::<GoalSet>(e).is_some_and(|g| !g.slot_running(inst.slot)) {
        hostile::arm_melee_after(w, e);
    }
}
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    hostile::melee_target(w, e).is_some()
}
fn start(w: &mut World, e: Entity, _: Inst) {
    if let Some(p) = hostile::melee_target(w, e) {
        hostile::start_melee(w, e, p);
    }
}
fn stop(w: &mut World, e: Entity, _: Inst) {
    hostile::stop_melee(w, e);
}
