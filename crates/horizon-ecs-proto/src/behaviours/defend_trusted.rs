//! Defence of a trusted entity (`Fox.DefendTrustedTargetGoal`): take as target whoever last hurt one of the `Trust` entities, and while
//! the goal runs raise the `raise` pose bit. Only a new hurt of that entity (its timestamp) starts the goal again.
use super::fox_state as pose;
use super::trust::Trust;
use super::*;
use crate::goals::TARGET;
use crate::cow::{Clock, LastHurtBy};
use crate::hostile::PlayerState;
use crate::targeting::{self, Target, Who};
use crate::rules::defend_trusted::{hit_is_fresh, target_memory, within_reach};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// The range of the attacker, in blocks (the follow range of the animal).
    pub follow_range: f64,
    /// `unseenMemoryTicks` of the target.
    pub unseen_memory: i32,
    /// Pose bit raised while defending, and the bits lowered when it starts (the species' flags).
    pub raise: u8,
    pub lower: u8,
    /// The sound at the start (the species' aggro sound), at the feet.
    pub sound: &'static str,
}

/// The one instance of the goal: its slot, the attacker it started on and the hurt timestamp it last answered.
#[derive(Component, Clone, Copy, Debug)]
pub struct DefendTrusted {
    pub p: &'static Params,
    pub slot: u8,
    pub attacker: Option<Who>,
    /// Sentinel `u64::MAX` until the first start, so the first hurt is new.
    pub timestamp: u64,
    hurt_at: u64,
}

pub static MODULE: Module = Module { name: "defend_trusted", flags: TARGET, equip, can_use, start, stop, requires: &[Cap::Targets], ..Module::DEFAULT };

fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let p = params::<Params>(p);
    w.entity_mut(e).insert(DefendTrusted { p, slot: inst.slot, attacker: None, timestamp: u64::MAX, hurt_at: 0 });
}

/// The attacker of a trusted entity and the tick it hit: 100 ticks for a player (as `LivingEntity.tick` keeps it).
fn hurt_by(w: &World, who: Who) -> Option<(Who, u64)> {
    match who {
        Who::Player(id) => {
            let r = w.resource::<PlayerState>().hurt_by.get(id as usize).copied().flatten()?;
            hit_is_fresh(w.resource::<Clock>().0, r.1).then_some(r)
        }
        Who::Entity(t) => w.get::<LastHurtBy>(t).map(|h| (h.who, h.at)),
    }
}

/// The animal is defending (its `raise` bit): the avoidance of a wolf and a polar bear is not started meanwhile.
pub fn is_defending(w: &World, e: Entity) -> bool {
    w.get::<DefendTrusted>(e).is_some_and(|d| pose::has(w, e, d.p.raise))
}

/// The attacker can be targeted, is not trusted itself and is within `follow_range`. DOUBT: the line of sight (`sees`) is not checked.
fn can_defend(w: &World, e: Entity, who: Who, range: f64) -> bool {
    let Some(t) = targeting::info(w, who) else { return false };
    if t.exempt || w.get::<Trust>(e).is_some_and(|tr| tr.trusts(who)) {
        return false;
    }
    let me = pose::feet(w, e);
    within_reach(t.pos.x - me.x, t.pos.y - me.y, t.pos.z - me.z, range)
}

fn can_use(w: &mut World, e: Entity, inst: Inst) -> bool {
    let Some(g) = w.get::<DefendTrusted>(e).copied() else { return false };
    if g.slot != inst.slot {
        return false;
    }
    // `NearestAttackableTargetGoal`'s random interval: one tick in `reduced(10)` asks.
    if crate::goals::with_rng(w, e, |r| r.next_int((10 + 1) / 2)) != 0 {
        return false;
    }
    let Some(trusted) = w.get::<Trust>(e).map(|t| t.0) else { return false };
    // The first trusted entity the level still knows decides, as vanilla returns from its loop.
    for who in trusted.into_iter().flatten() {
        let Some((by, at)) = hurt_by(w, who) else { continue };
        if let Some(mut d) = w.get_mut::<DefendTrusted>(e) {
            d.attacker = Some(by);
            d.hurt_at = at;
        }
        return at != g.timestamp && can_defend(w, e, by, g.p.follow_range);
    }
    false
}

fn start(w: &mut World, e: Entity, inst: Inst) {
    let Some(g) = w.get::<DefendTrusted>(e).copied() else { return };
    if g.slot != inst.slot {
        return;
    }
    if let Some(mut d) = w.get_mut::<DefendTrusted>(e) {
        d.timestamp = d.hurt_at;
    }
    if let Some(by) = g.attacker {
        targeting::set_target(w, e, Target::new(by, target_memory(g.p.unseen_memory), inst.slot));
    }
    let at = pose::feet(w, e);
    w.resource_mut::<crate::goals::Events>().host.sound(g.p.sound, [at.x, at.y, at.z], 1.0, 1.0);
    pose::set(w, e, g.p.raise, true);
    pose::set(w, e, g.p.lower, false);
}

fn stop(w: &mut World, e: Entity, inst: Inst) {
    if w.get::<Target>(e).is_some_and(|t| t.slot == inst.slot) {
        targeting::clear_target(w, e);
    }
    if let Some(mut d) = w.get_mut::<DefendTrusted>(e) {
        d.attacker = None;
    }
}
