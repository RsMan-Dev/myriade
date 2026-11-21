//! The area beam (`SonicBoom`): with a target within `range_h` blocks horizontally and `range_v` vertically, and no cooldown, the entity
//! plays the charge at once, then after `sound_delay` ticks draws the beam from its chest to the target's eyes, plays the boom and
//! lands the blow (the damage and the knockback are the frame's, a socle). The act lasts `duration` ticks; the boom is the first
//! tick of the deadline of `sound_delay`, the end the deadline of `duration`.
use super::Inst;
use super::act_phase::{self, ActPhase, Phase};
use super::{params, Module};
use crate::rules::sonic_boom as rules;
use crate::{cow::*, goals::*, targeting::{self, Target, Who}};
use bevy_ecs::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub duration: i32,
    pub sound_delay: i32,
    /// Ticks before the next beam (the Mob sets it to the act's duration).
    pub cooldown: u64,
    /// The cooldown a new target starts (`setAttackTarget`: 200).
    pub first_cooldown: u64,
    /// The reach: `range_h` horizontal and `range_v` vertical, in blocks.
    pub range_h: f64,
    pub range_v: f64,
    /// Height of the chest, where the beam starts.
    pub chest: f64,
    pub charge_sound: &'static str,
    pub boom_sound: &'static str,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct Cfg(pub &'static Params);

pub static MODULE: Module = Module {
    name: "sonic_boom",
    flags: LOOK,
    resources,
    equip,
    can_use,
    start,
    stop,
    fired,
    interruptable: false,
    ..Module::DEFAULT
};

/// A new target starts the cooldown (`Warden.setAttackTarget`: 200 ticks) and the goal is asked when it ends.
fn resources(w: &mut World) {
    w.resource_mut::<targeting::TargetWatchers>().0.push(|w, e| {
        let Some(c) = w.get::<Cfg>(e).map(|c| c.0) else { return };
        let now = now_of(w, e);
        w.get_mut::<ActPhase>(e).unwrap().sonic_cooldown_until = now + c.first_cooldown;
        arm_module(w, e, &MODULE, c.first_cooldown as i32);
    });
}
fn equip(w: &mut World, e: Entity, _: Inst, p: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Cfg(params::<Params>(p)));
    w.entity_mut(e).insert_if_new(ActPhase::default());
}

fn within(w: &World, e: Entity, who: Who, c: &Params) -> bool {
    let (Some(i), Some(p)) = (targeting::info(w, who), w.get::<Pos>(e)) else { return false };
    rules::in_reach(i.pos.x - p.0.x, i.pos.y - p.0.y, i.pos.z - p.0.z, c.range_h, c.range_v)
}

fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    let now = now_of(w, e);
    let c = w.get::<Cfg>(e).unwrap().0;
    let Some(target) = w.get::<Target>(e).map(|t| t.who) else { return false };
    let until = w.get::<ActPhase>(e).unwrap().sonic_cooldown_until;
    let ok = now >= until && targeting::can_target(w, e, target) && within(w, e, target, &c);
    if !ok {
        // The brain asks every tick: here, at the end of the cooldown, then every 5 ticks while the target is out of reach.
        arm_module(w, e, &MODULE, if now < until { (until - now) as i32 } else { 5 });
    }
    ok
}

fn start(w: &mut World, e: Entity, inst: Inst) {
    let c = w.get::<Cfg>(e).unwrap().0;
    let now = now_of(w, e);
    let mut st = w.get_mut::<ActPhase>(e).unwrap();
    st.phase = Phase::Booming;
    st.until = now + c.duration as u64;
    st.sonic_cooldown_until = now + c.cooldown;
    st.boomed = false;
    drop(st);
    act_phase::sound(w, e, c.charge_sound, 3.0, 1.0);
    arm(w, e, inst.slot, c.sound_delay);
}

/// The boom at the sound delay, then the end at the deadline of the act.
fn fired(w: &mut World, e: Entity, inst: Inst) -> bool {
    // Not in the act: the deadline is the ask for a start.
    if w.get::<ActPhase>(e).unwrap().phase != Phase::Booming {
        return true;
    }
    if !w.get::<ActPhase>(e).unwrap().boomed {
        let now = now_of(w, e);
        let until = w.get::<ActPhase>(e).unwrap().until;
        boom(w, e);
        w.get_mut::<ActPhase>(e).unwrap().boomed = true;
        arm(w, e, inst.slot, until.saturating_sub(now) as i32);
        return false;
    }
    act_phase::deadline(w, e, &MODULE)
}

fn boom(w: &mut World, e: Entity) {
    let c = w.get::<Cfg>(e).unwrap().0;
    let Some(target) = w.get::<Target>(e).map(|t| t.who) else { return };
    let Some(i) = targeting::info(w, target) else { return };
    let p = w.get::<Pos>(e).unwrap().0;
    let src = [p.x, p.y + c.chest, p.z];
    let eye = [i.pos.x, i.eye, i.pos.z];
    let (n, len) = rules::beam(src, eye);
    for k in 1..rules::beam_steps(len) {
        let at = rules::beam_point(src, n, k);
        w.resource_mut::<Events>().host.particles("sonic_boom", at, [0.0; 3], 0.0, 0);
    }
    act_phase::sound(w, e, c.boom_sound, 3.0, 1.0);
    crate::health::sonic_boom(w, e, target, n);
}

fn stop(w: &mut World, e: Entity, _: Inst) {
    w.get_mut::<ActPhase>(e).unwrap().phase = Phase::Standing;
}
