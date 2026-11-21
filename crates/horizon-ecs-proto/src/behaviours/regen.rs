//! The wither's regeneration (`WitherBoss.aiStep`, `age % 20`): 1 health every 20 ticks, and while `boost.when` holds (the charge)
//! 10 every 10 ticks. One wheel deadline per animal (`Due::Goal`), re-armed by each heal: no tick. A boost that starts or ends
//! between two heals takes effect at the next one.
use super::*;
use crate::behaviours::{boss_bar, invulnerable_charge};
use crate::cow::*;
use crate::goals::*;

#[derive(Clone, Copy, Debug)]
pub struct Boost {
    pub period: u64,
    pub amount: f32,
    pub when: fn(&World, Entity) -> bool,
}
#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub period: u64,
    pub amount: f32,
    pub boost: Option<Boost>,
}
impl Params {
    pub const WITHER: Self = Self {
        period: 20,
        amount: 1.0,
        boost: Some(Boost { period: 10, amount: 10.0, when: invulnerable_charge::charging }),
    };
}
#[derive(Component, Clone, Copy, Debug)]
pub struct Regen {
    p: &'static Params,
    slot: u8,
    due: u64,
    asleep: bool,
}
pub static MODULE: Module = Module {
    name: "regen",
    flags: 0,
    equip,
    damaged,
    fired,
    can_use,
    start,
    requires: &[Cap::Body, Cap::Living],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let p = params::<Params>(p);
    let due = now_of(w, e) + p.period;
    w.entity_mut(e).insert(Regen { p, slot: inst.slot, due, asleep: false });
    arm_at(w, e, due, Due::Goal(inst.slot));
}
fn fired(w: &mut World, e: Entity, _: Inst) -> bool {
    w.get::<Regen>(e).is_some_and(|r| now_of(w, e) >= r.due)
}
fn can_use(_: &mut World, _: Entity, _: Inst) -> bool {
    true
}
fn start(w: &mut World, e: Entity, inst: Inst) {
    let now = now_of(w, e);
    let Some(r) = w.get::<Regen>(e).copied() else { return };
    if now < r.due {
        return;
    }
    let boosted = r.p.boost.filter(|b| (b.when)(w, e));
    let (period, amount) = boosted.map_or((r.p.period, r.p.amount), |b| (b.period, b.amount));
    heal(w, e, amount);
    let due = now + period;
    w.get_mut::<Regen>(e).unwrap().due = due;
    // At full health the heal does nothing: sleep, the next damage re-arms on the same phase (`damaged`).
    if full(w, e) {
        w.get_mut::<Regen>(e).unwrap().asleep = true;
    } else {
        arm_at(w, e, due, Due::Goal(r.slot));
    }
    w.resource_mut::<Requests>().0.push(Req::End(e, inst.slot));
}
fn heal(w: &mut World, e: Entity, amount: f32) {
    {
        let Some(cur) = w.get::<Health>(e).map(|h| h.0) else { return };
        let max = w.get::<MaxHealth>(e).map_or(cur, |m| m.0);
        let mut h = w.get_mut::<Health>(e).unwrap();
        h.0 = (h.0 + amount).min(max);
    }
    boss_bar::progress(w, e);
}
fn full(w: &World, e: Entity) -> bool {
    let Some(h) = w.get::<Health>(e) else { return true };
    w.get::<MaxHealth>(e).is_none_or(|m| h.0 >= m.0)
}
/// Damage wakes a sleeping regeneration at the next heal date of its phase (`age % period`).
fn damaged(w: &mut World, e: Entity, inst: Inst, _: &Damaged) {
    let now = now_of(w, e);
    let Some(mut r) = w.get::<Regen>(e).copied() else { return };
    if !r.asleep || full(w, e) {
        return;
    }
    let step = r.p.period.max(1);
    r.asleep = false;
    r.due += (now.saturating_sub(r.due)) / step * step + if now.saturating_sub(r.due) % step == 0 { 0 } else { step };
    *w.get_mut::<Regen>(e).unwrap() = r;
    arm_at(w, e, r.due, Due::Goal(r.slot));
}
