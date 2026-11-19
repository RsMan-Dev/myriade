//! `HurtByTargetGoal`: whoever hurt the attacker last (within 100 ticks, once per hit) becomes its target, and the
//! creatures of the same kind around it that have no target are alerted (`alertOthers`). What wakes it: the damage event
//! (`apply_damage` writes `HurtBy`), heard on the selector's next tick; nothing else, no polling.
use super::*;
use crate::targeting::{self, FollowRange, Target, Who};
use crate::{cow::*, goals::*, hostile::HurtBy};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// `setAlertOthers`: the creatures around take the same target.
    pub alert: bool,
    /// The kinds alerted (empty: the attacker's own kind), and the kinds never alerted.
    pub alert_types: &'static [&'static str],
    pub alert_except: &'static [&'static str],
    /// `unseenMemoryTicks` of the target it takes, and of the one it gives to the alerted.
    pub unseen_memory: i32,
    /// The owner (`tame::Owner`) is never taken for a target, whoever hit.
    pub spare_owner: bool,
    /// `alert_if`: a condition the ALERTED creature must meet (the panda: only the aggressive ones are called).
    pub alert_when: Option<fn(&World, Entity) -> bool>,
}
impl Params {
    pub const DEFAULT: Self = Self { alert: false, alert_types: &[], alert_except: &[], unseen_memory: 300, spare_owner: false, alert_when: None };
    pub const ALERT: Self = Self { alert: true, ..Self::DEFAULT };
}
/// The state: the last hit (who, tick) and the one the goal answered, the kinds resolved to masks.
#[derive(Component, Clone, Copy, Debug)]
pub struct HurtByGoal {
    p: Params,
    pub slot: u8,
    last: Option<(Who, u64)>,
    handled: u64,
    alert_types: crate::cow::SpeciesSet,
    alert_except: crate::cow::SpeciesSet,
    /// A target an alerted creature takes (written by the one that called it).
    called_by: Option<Who>,
}

pub static MODULE: Module = Module {
    name: "hurt_by",
    flags: TARGET,
    equip,
    can_use,
    start,
    stop,
    systems,
    requires: &[Cap::Living, Cap::Targets],
    ..Module::DEFAULT
};

fn mask_of(w: &World, names: &[&str]) -> crate::cow::SpeciesSet {
    let _ = w;
    names.iter().filter_map(|n| crate::species::registry().id(n)).fold(crate::cow::SpeciesSet::EMPTY, |m, i| m.with(i as usize))
}
fn equip(w: &mut World, e: Entity, inst: Inst, p: &(dyn std::any::Any + Sync)) {
    let p = *params::<Params>(p);
    let (alert_types, alert_except) = (mask_of(w, p.alert_types), mask_of(w, p.alert_except));
    w.entity_mut(e).insert(HurtByGoal { p, slot: inst.slot, last: None, handled: u64::MAX, alert_types, alert_except, called_by: None });
}
fn systems(s: &mut Schedule, slot: GoalSlot) {
    s.add_systems(hurt_events.after(apply_damage).in_set(slot));
}
/// A creature was hurt by a player or a creature: the goal is woken at the selector's next tick.
fn hurt_events(clock: Res<Clock>, mut by: ResMut<HurtBy>, mut q: Query<&mut HurtByGoal>, mut reqs: ResMut<Requests>) {
    for (e, who) in std::mem::take(&mut by.0) {
        if let Ok(mut g) = q.get_mut(e) {
            g.last = Some((who, clock.of(e)));
            reqs.0.push(Req::Start(e, g.slot));
        }
    }
}
/// `HurtByTargetGoal.canUse`: a hit it has not answered yet, by someone alive that can be targeted.
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    let now = crate::cow::now_of(w, e);
    let Some(g) = w.get::<HurtByGoal>(e).copied() else { return false };
    let Some((who, at)) = g.last else { return false };
    // `getLastHurtByMob` is forgotten after 100 ticks.
    if at == g.handled || now.saturating_sub(at) > 100 {
        return false;
    }
    if g.p.spare_owner && w.get::<crate::tame::Owner>(e).is_some_and(|o| o.0 == Some(who)) {
        return false;
    }
    match targeting::info(w, who) {
        Some(i) => !i.exempt,
        None => false,
    }
}
fn start(w: &mut World, e: Entity, inst: Inst) {
    let Some(mut g) = w.get::<HurtByGoal>(e).copied() else { return };
    let Some((who, at)) = g.last else { return };
    g.handled = at;
    w.get_mut::<HurtByGoal>(e).unwrap().handled = at;
    let memory = ((g.p.unseen_memory + 1) / 2) as u16;
    targeting::set_target(w, e, Target::new(who, memory, inst.slot));
    if g.p.alert {
        alert_others(w, e, who, g);
    }
}
/// `HurtByTargetGoal.alertOthers`: the others of the alerted kinds within the follow range (10 blocks up and down) that
/// have no target take the same one.
fn alert_others(w: &mut World, e: Entity, who: Who, g: HurtByGoal) {
    let (pos, sid) = (w.get::<Pos>(e).unwrap().0, w.get::<SpeciesId>(e).unwrap().0);
    let follow = w.get::<FollowRange>(e).map_or(16.0, |f| f.0);
    let kinds = if g.alert_types.is_empty() { crate::cow::SpeciesSet::EMPTY.with(usize::from(sid)) } else { g.alert_types };
    let mut others = Vec::new();
    w.resource::<Index>().for_radius(pos.x, pos.z, follow, |o| others.push(o));
    for o in others {
        if o == e || Who::Entity(o) == who || w.get::<Target>(o).is_some() {
            continue;
        }
        let (Some(op), Some(os), true) = (w.get::<Pos>(o), w.get::<SpeciesId>(o), w.get::<HurtByGoal>(o).is_some()) else { continue };
        if kinds.has(os.0)
            && !g.alert_except.has(os.0)
            && g.p.alert_when.is_none_or(|f| f(w, o))
            && (op.0.x - pos.x).abs() <= follow + 1.0
            && (op.0.z - pos.z).abs() <= follow + 1.0
            && (op.0.y - pos.y).abs() <= 11.0
        {
            w.get_mut::<HurtByGoal>(o).unwrap().called_by = Some(who);
            w.resource_mut::<Requests>().0.push(Req::Call(o, called));
        }
    }
}
/// An alerted creature takes the target of the one that called it: `MobEvent::SetTarget` writes the target of the creature
/// without any of its goals running, so it keeps it until the target dies (no memory, no follow distance), and a target
/// goal of the creature can still replace it.
fn called(w: &mut World, e: Entity) {
    let Some(g) = w.get::<HurtByGoal>(e).copied() else { return };
    let Some(who) = g.called_by else { return };
    if w.get::<Target>(e).is_some() {
        return;
    }
    w.get_mut::<HurtByGoal>(e).unwrap().called_by = None;
    let mut t = Target::new(who, u16::MAX, u8::MAX);
    (t.must_see, t.follow_scale) = (false, f64::INFINITY);
    targeting::set_target(w, e, t);
}
fn stop(w: &mut World, e: Entity, inst: Inst) {
    if w.get::<Target>(e).is_some_and(|t| t.slot == inst.slot) {
        targeting::clear_target(w, e);
    }
}
