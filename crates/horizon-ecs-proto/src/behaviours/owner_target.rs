//! `OwnerHurtByTargetGoal` and `OwnerHurtTargetGoal`: a tame pet that is not ordered to sit attacks what hurt its owner (a player), or what its
//! owner hit last. What it never attacks is a parameter (kinds, kinds that are spared when they are tame, and what has the same owner).
//! What wakes them: the owner's combat record changing (`tame::owner_events`), nothing else.
use super::*;
use crate::targeting::{self, Target, Who};
use crate::small::Nth;
use crate::{cow::*, goals::*, hostile::PlayerState, tame};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// Which record of the owner: the one that hurt it (`getLastHurtByMob`, within 100 ticks), or the one it hurt (`getLastHurtMob`).
    pub hurt_by: bool,
    /// `Wolf.wantsToAttack`: the kinds never attacked, the kinds spared while they are tame, and whether what has the same owner is spared.
    pub never: &'static [&'static str],
    pub spare_when_tame: &'static [&'static str],
    pub spare_same_owner: bool,
    /// `unseenMemoryTicks` of the target.
    pub unseen_memory: i32,
}
impl Params {
    pub const HURT_BY: Self = Self { hurt_by: true, never: &[], spare_when_tame: &[], spare_same_owner: false, unseen_memory: 60 };
    pub const HURT: Self = Self { hurt_by: false, ..Self::HURT_BY };
}
#[derive(Clone, Copy, Debug)]
pub struct OwnerTarget {
    pub p: Params,
    pub slot: u8,
    handled: u64,
}
/// The instances of the pet, found by their rank in the selector.
#[derive(Component, Clone, Debug)]
pub struct OwnerTargets(pub Nth<OwnerTarget, 2>);

pub static MODULE: Module = Module { name: "owner_target", flags: TARGET, equip, can_use, start, stop, requires: &[Cap::Tamable, Cap::Targets], ..Module::DEFAULT };
fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let t = OwnerTarget { p: *params::<Params>(p), slot: inst.slot, handled: u64::MAX };
    match w.get_mut::<OwnerTargets>(e) {
        Some(mut v) => v.0.push(t),
        None => {
            w.entity_mut(e).insert(OwnerTargets(Nth::one(t)));
        }
    }
}
fn state(w: &World, e: Entity, inst: Inst) -> Option<OwnerTarget> {
    w.get::<OwnerTargets>(e)?.0.iter().find(|t| t.slot == inst.slot).copied()
}
/// The record of the owner that this instance answers: who, when.
fn record(w: &World, e: Entity, p: &Params) -> Option<(Who, u64)> {
    let Some(Who::Player(id)) = w.get::<tame::Owner>(e)?.0 else { return None };
    let ps = w.resource::<PlayerState>();
    let r = if p.hurt_by { ps.hurt_by.get(id as usize) } else { ps.last_hurt.get(id as usize) }.copied().flatten()?;
    let now = w.resource::<Clock>().0;
    (!p.hurt_by || now.saturating_sub(r.1) <= 100).then_some(r)
}
fn can_use(w: &mut World, e: Entity, inst: Inst) -> bool {
    let Some(st) = state(w, e, inst) else { return false };
    if is_occupied(w, e, OCC_SITTING) {
        return false;
    }
    let Some((who, at)) = record(w, e, &st.p) else { return false };
    at != st.handled && wants_to_attack(w, e, who, &st.p)
}
fn start(w: &mut World, e: Entity, inst: Inst) {
    let Some(st) = state(w, e, inst) else { return };
    let Some((who, at)) = record(w, e, &st.p) else { return };
    if let Some(mut v) = w.get_mut::<OwnerTargets>(e)
        && let Some(t) = v.0.iter_mut().find(|t| t.slot == inst.slot)
    {
        t.handled = at;
    }
    targeting::set_target(w, e, Target::new(who, ((st.p.unseen_memory + 1) / 2) as u16, inst.slot));
}
fn stop(w: &mut World, e: Entity, inst: Inst) {
    if w.get::<Target>(e).is_some_and(|t| t.slot == inst.slot) {
        targeting::clear_target(w, e);
    }
}
/// `Wolf.wantsToAttack`.
fn wants_to_attack(w: &World, e: Entity, who: Who, p: &Params) -> bool {
    let Who::Entity(t) = who else { return false };
    if t == e || targeting::info(w, who).is_none() {
        return false;
    }
    let kind = w.get::<TypeName>(t).map_or("", |n| n.0);
    if p.never.contains(&kind) {
        return false;
    }
    let theirs = w.get::<tame::Owner>(t).and_then(|o| o.0);
    if theirs.is_some() && p.spare_when_tame.contains(&kind) {
        return false;
    }
    !(p.spare_same_owner && theirs.is_some() && theirs == w.get::<tame::Owner>(e).and_then(|o| o.0))
}
