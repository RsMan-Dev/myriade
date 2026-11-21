//! `Ravager.aiStep` speed (`ravager.rs:98-111`): the base moves 10 % of the way to 0.35 while it has a target, else to 0.3, each
//! tick; while busy (`BUSY`) the base is 0. Not ticked: the state is `(base, since, max, busy)`, written when the target or the busy
//! reasons change (`TargetWatchers`), and `speed_at` closes the formula over the ticks elapsed since, exact for the 10 % step.
use super::*;
use crate::rules::hunt_speed::{pace_max, paced_speed};
use crate::{goals::*, targeting::TargetWatchers};

/// The busy reasons that stop a ravager (`Mob.isImmobile`): biting, stunned, roaring.
pub const BUSY: u32 = super::strike_pause::OCC_BITING | super::shield_stun::OCC_STUNNED | super::ravager_roar::OCC_ROARING;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub hunting: f64,
    pub idle: f64,
}
impl Params {
    pub const DEFAULT: Self = Self { hunting: 0.35, idle: 0.3 };
}
/// `base` at tick `since`, approaching `max` (or 0 while busy).
#[derive(Component, Clone, Copy, Debug)]
pub struct Pace {
    base: f64,
    since: u64,
    max: f64,
    busy: bool,
    hunting: f64,
    idle: f64,
}

pub static MODULE: Module = Module {
    name: "hunt_speed",
    flags: 0,
    equip,
    resources,
    requires: &[Cap::Body, Cap::Locomotion],
    ..Module::DEFAULT
};
fn resources(w: &mut World) {
    w.resource_mut::<TargetWatchers>().0.push(on_target);
}
fn equip(w: &mut World, e: Entity, _: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let p = *params::<Params>(p);
    let now = w.resource::<crate::cow::Clock>().0;
    w.entity_mut(e).insert(Pace { base: p.idle, since: now, max: p.idle, busy: false, hunting: p.hunting, idle: p.idle });
}
/// A target was given or taken: settle the base to now, then aim at the new mode.
fn on_target(w: &mut World, e: Entity) {
    if w.get::<Pace>(e).is_none() {
        return;
    }
    refresh(w, e);
}
/// Writes the mode of now (the target and the busy reasons) after settling the base.
pub fn refresh(w: &mut World, e: Entity) {
    let now = w.resource::<crate::cow::Clock>().0;
    let busy = is_occupied(w, e, BUSY);
    let hunting = w.get::<crate::targeting::Target>(e).is_some();
    let Some(mut p) = w.get_mut::<Pace>(e) else { return };
    p.base = speed_from(&p, now);
    p.since = now;
    p.busy = busy;
    p.max = pace_max(hunting, p.hunting, p.idle);
}
/// The speed the locomotion moves at this tick.
pub fn speed_at(w: &World, e: Entity) -> f64 {
    let (Some(p), now) = (w.get::<Pace>(e), w.resource::<crate::cow::Clock>().0) else { return 0.0 };
    if p.busy { 0.0 } else { speed_from(p, now) }
}
fn speed_from(p: &Pace, now: u64) -> f64 {
    paced_speed(p.base, p.max, p.busy, now.saturating_sub(p.since))
}
