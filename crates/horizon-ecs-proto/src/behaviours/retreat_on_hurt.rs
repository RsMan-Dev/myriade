//! `HoglinAi.wasHurtBy` for a baby (`piglin.rs:851-865`, `retreatFromNearestTarget` + `setAvoidTarget`): a hit makes it forget its attack target
//! and its walk and keep away from the nearest of the attacker, its avoid target and its attack target, for 100 to 400 ticks
//! (`AVOID_TARGET`, read by `walk_away` and by the goals `.when(NOT_RETREATING)`). Woken by the damage event only (`Events.hurt_by_entity`), no polling.
use super::*;
use crate::brain::{self, behaviours::walk_away::AVOID_TARGET, MemValue};
use crate::targeting::{self, Target, Who};
use crate::{cow::*, goals::*};

#[derive(Component, Clone, Copy, Debug)]
pub struct Retreat {
    slot: u8,
    handled: u64,
}

pub static MODULE: Module = Module {
    name: "retreat_on_hurt",
    flags: 0,
    equip,
    can_use,
    start,
    systems,
    requires: &[Cap::Living, Cap::Targets, Cap::Brain],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, inst: Inst, _: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Retreat { slot: inst.slot, handled: u64::MAX });
}
fn systems(s: &mut Schedule, slot: GoalSlot) {
    s.add_systems(hit.in_set(slot));
}
fn hit(ev: Res<Events>, q: Query<&Retreat>, mut reqs: ResMut<Requests>) {
    for e in ev.hurt_by_entity.iter() {
        if let Ok(r) = q.get(e) {
            reqs.0.push(Req::Start(e, r.slot));
        }
    }
}
/// A baby, hit by someone who still exists, not yet answered (`LastHurtBy` is the hit, written by `apply_damage`).
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    let Some(r) = w.get::<Retreat>(e).copied().filter(|_| w.get::<GrowAt>(e).is_some()) else { return false };
    matches!(w.get::<LastHurtBy>(e), Some(h) if h.at != r.handled && targeting::info(w, h.who).is_some())
}
fn start(w: &mut World, e: Entity, _: Inst) {
    let Some(mut r) = w.get::<Retreat>(e).copied() else { return };
    let Some(h) = w.get::<LastHurtBy>(e).copied() else { return };
    let who = h.who;
    r.handled = h.at;
    w.entity_mut(e).insert(r);
    retreat_from_nearest(w, e, who);
}
/// `HoglinAi.retreatFromNearestTarget`: keep away from the nearest of `who`, the avoid target and the attack target.
pub fn retreat_from_nearest(w: &mut World, e: Entity, who: Who) {
    let Some(me) = w.get::<Pos>(e).map(|p| p.0) else { return };
    let d2 = |p: horizon_core::geometry::Vec3| (p.x - me.x).powi(2) + (p.y - me.y).powi(2) + (p.z - me.z).powi(2);
    let mut best = (who, targeting::info(w, who).map_or(f64::MAX, |i| d2(i.pos)));
    let attack = w.get::<Target>(e).map(|t| t.who);
    let avoid = match brain::get(w, e, &AVOID_TARGET) {
        Some(MemValue::Who(a)) => Some(*a),
        _ => None,
    };
    for other in [avoid, attack].into_iter().flatten() {
        if let Some(i) = targeting::info(w, other)
            && d2(i.pos) < best.1
        {
            best = (other, d2(i.pos));
        }
    }
    avoid_target(w, e, best.0);
}
/// `HoglinAi.setAvoidTarget`: forget the attack target and the walk, avoid `who` for 100 to 400 ticks.
pub fn avoid_target(w: &mut World, e: Entity, who: Who) {
    targeting::clear_target(w, e);
    stop_nav(w, e);
    let ticks = 100 + with_rng(w, e, |r| r.next_int(301)) as u64;
    brain::set(w, e, &AVOID_TARGET, MemValue::Who(who), Some(ticks));
}
