//! `NearestAttackableTargetGoal` and `HurtByTargetGoal`: the target selector of a hostile monster. No flags; it picks
//! the player to hunt (`Aggro`), which the melee goal then chases. The ask is a deadline of the wheel (1 chance in 5 per
//! ask, one ask every 2 ticks, sampled once).
use super::*;
use crate::{cow::*, goals::*, hostile};

#[derive(Clone, Copy, Debug)]
pub struct Params {}
impl Params {
    pub const DEFAULT: Self = Self {};
}
/// `armed`: its deadline is in the wheel; `pick`: the player `can_use` chose.
#[derive(Component, Clone, Copy, Debug)]
pub struct TargetGoal {
    pub armed: bool,
    pub slot: u8,
    pick: u32,
}

pub static MODULE: Module = Module {
    name: "target",
    flags: 0,
    ends_with_path: false,
    blocked: None,
    retry_on_end: false,
    after_change: None,
    resources: no_resources,
    equip,
    arm,
    fired,
    can_use,
    start,
    stop,
    systems,
};
fn equip(w: &mut World, e: Entity, inst: Inst, _: &(dyn std::any::Any + Sync)) {
    hostile::ensure_attack(w, e);
    w.get_mut::<hostile::Attack>(e).unwrap().target_slot = inst.slot;
    w.entity_mut(e).insert(TargetGoal { armed: false, slot: inst.slot, pick: 0 });
}
fn systems(s: &mut Schedule, slot: GoalSlot) {
    hostile::target_systems(s, slot);
}
/// The target selector asks every other tick, 1 in `reduced(10)` = 5.
fn arm(w: &mut World, e: Entity, inst: Inst) {
    let Some(t) = w.get::<TargetGoal>(e).copied() else { return };
    if !t.armed {
        let wait = with_rng(w, e, |r| 2 * geometric(r, 5));
        crate::goals::arm(w, e, inst.slot, wait);
        w.get_mut::<TargetGoal>(e).unwrap().armed = true;
    }
}
/// The ask is lost or the target is: the next ask is armed again.
pub fn unarm(w: &mut World, e: Entity) {
    if let Some(mut t) = w.get_mut::<TargetGoal>(e) {
        t.armed = false;
    }
}
fn fired(w: &mut World, e: Entity, _: Inst) -> bool {
    let now = w.resource::<Clock>().0;
    let active = w.get::<ActiveUntil>(e).map_or(0, |a| a.0);
    let Some(mut t) = w.get_mut::<TargetGoal>(e) else { return false };
    t.armed = false;
    // Out of the active zone the selector does not ask (the next entry arms it again); with a target it runs.
    !(now >= active || w.get::<hostile::Aggro>(e).is_some())
}
fn can_use(w: &mut World, e: Entity, inst: Inst) -> bool {
    match hostile::pick_target(w, e) {
        Some(p) => {
            w.get_mut::<TargetGoal>(e).unwrap().pick = p;
            true
        }
        None => {
            unarm(w, e);
            arm(w, e, inst);
            false
        }
    }
}
fn start(w: &mut World, e: Entity, _: Inst) {
    let p = w.get::<TargetGoal>(e).unwrap().pick;
    hostile::acquire(w, e, p);
}
fn stop(w: &mut World, e: Entity, _: Inst) {
    hostile::lose_target(w, e);
}
