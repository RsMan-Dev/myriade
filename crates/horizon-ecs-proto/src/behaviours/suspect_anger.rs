//! Anger per suspect and the target it gives (`AngerManagement` and the target part of `Warden.handleHurt` in `mobs/warden.rs`).
//! Each suspect's anger is capped at `max` and loses one point per `step` ticks of own time while the suspect can still be targeted;
//! it is settled when read, from the time it was set (no timer, no scan). `raise` is called by what angers it (a vibration, a roar,
//! a hit) and asks the target goal again. The values come from the species (`Params`), none is written here.
use super::Inst;
use super::act_phase::{ActPhase, RareState};
use super::{Damaged, Module, Cap, params};
use crate::{cow::*, goals::*, targeting::{self, Target, Who}};
use bevy_ecs::prelude::*;
use std::cmp::Reverse;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// The most anger one suspect can hold.
    pub max: u16,
    /// Levels: agitated from `agitated`, angry from `angry`.
    pub agitated: u16,
    pub angry: u16,
    /// Ticks of own time per point of anger lost.
    pub step: u64,
    /// How long the target keeps the suspect without seeing it (`Target.memory`).
    pub memory: u16,
    /// The anger a hit adds on top of the angry level.
    pub hurt_boost: u16,
    /// Ticks after an anger during which the dig cannot start.
    pub dig_cooldown: u64,
    /// A hit from this close (squared distance) also makes it the target.
    pub hurt_target_range_sqr: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Suspect {
    pub who: Who,
    pub anger: u16,
    pub at: u64,
}

#[derive(Component, Clone, Debug)]
pub struct Suspects {
    pub list: Vec<Suspect>,
    pub p: Params,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct Cfg(pub Params);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Calm,
    Agitated,
    Angry,
}

pub static MODULE: Module = Module {
    name: "suspect_anger",
    flags: 0,
    equip,
    can_use,
    start,
    damaged,
    requires: &[Cap::Targets],
    ..Module::DEFAULT
};

fn equip(w: &mut World, e: Entity, _: Inst, p: &(dyn std::any::Any + Sync)) {
    let p = *params::<Params>(p);
    w.entity_mut(e).insert((Suspects { list: Vec::new(), p }, Cfg(p)));
    w.entity_mut(e).insert_if_new(ActPhase::default());
    w.entity_mut(e).insert_if_new(RareState::default());
}

/// The anger of a suspect at `now`: None when it has run out.
fn value(s: &Suspect, now: u64, p: &Params) -> Option<u16> {
    let steps = now.saturating_sub(s.at) / p.step;
    (steps < u64::from(s.anger)).then(|| s.anger - steps as u16)
}

/// The suspects that still count, highest anger first.
pub fn live(w: &World, e: Entity) -> Vec<(Who, u16)> {
    let now = now_of(w, e);
    let Some(s) = w.get::<Suspects>(e) else { return Vec::new() };
    let mut v: Vec<(Who, u16)> = s
        .list
        .iter()
        .filter_map(|x| value(x, now, &s.p).map(|a| (x.who, a)))
        .filter(|&(who, _)| targeting::can_target(w, e, who))
        .collect();
    v.sort_by_key(|&(_, a)| Reverse(a));
    v
}

/// The level of the highest live anger.
pub fn level(w: &World, e: Entity) -> Level {
    let Some(s) = w.get::<Suspects>(e) else { return Level::Calm };
    let a = live(w, e).first().map_or(0, |x| x.1);
    if a >= s.p.angry {
        Level::Angry
    } else if a >= s.p.agitated {
        Level::Agitated
    } else {
        Level::Calm
    }
}

/// `increaseAnger`: `who` gains `amount` (capped), the dig cooldown restarts, and the target is asked again. Nothing happens for a
/// suspect that cannot be targeted; returns the new anger.
pub fn raise(w: &mut World, e: Entity, who: Who, amount: u16) -> u16 {
    if !targeting::can_target(w, e, who) {
        return 0;
    }
    let now = now_of(w, e);
    let mut s = w.get_mut::<Suspects>(e).unwrap();
    let p = s.p;
    let anger = match s.list.iter().position(|x| x.who == who) {
        Some(i) => {
            let cur = value(&s.list[i], now, &p).unwrap_or(0);
            let a = cur.saturating_add(amount).min(p.max);
            s.list[i] = Suspect { who, anger: a, at: now };
            a
        }
        None => {
            let a = amount.min(p.max);
            s.list.push(Suspect { who, anger: a, at: now });
            a
        }
    };
    drop(s);
    w.get_mut::<ActPhase>(e).unwrap().dig_cooldown_until = now + p.dig_cooldown;
    arm_module(w, e, &MODULE, 1);
    anger
}

/// The target goal: the first live suspect, when it is not the target already.
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    let Some(&(who, _)) = live(w, e).first() else { return false };
    w.get::<Target>(e).map(|t| t.who) != Some(who)
}

fn start(w: &mut World, e: Entity, inst: Inst) {
    let Some(&(who, _)) = live(w, e).first() else { return };
    let memory = w.get::<Suspects>(e).unwrap().p.memory;
    targeting::set_target(w, e, Target::new(who, memory, inst.slot));
    w.resource_mut::<Requests>().0.push(Req::End(e, inst.slot));
}

/// `handleHurt`: whoever hits it is angered by the angry level and the boost. It becomes the target when it has none and the hit
/// was direct, or the hitter stands within `hurt_target_range_sqr`.
fn damaged(w: &mut World, e: Entity, inst: Inst, d: &Damaged) {
    let Some(who) = d.who else { return };
    let p = w.get::<Suspects>(e).unwrap().p;
    raise(w, e, who, p.angry + p.hurt_boost);
    if w.get::<Target>(e).is_some() {
        return;
    }
    let near = match (targeting::info(w, who), w.get::<Pos>(e)) {
        (Some(i), Some(pos)) => {
            let (dx, dy, dz) = (i.pos.x - pos.0.x, i.pos.y - pos.0.y, i.pos.z - pos.0.z);
            dx * dx + dy * dy + dz * dz < p.hurt_target_range_sqr
        }
        _ => false,
    };
    if d.direct || near {
        targeting::set_target(w, e, Target::new(who, p.memory, inst.slot));
    }
}
