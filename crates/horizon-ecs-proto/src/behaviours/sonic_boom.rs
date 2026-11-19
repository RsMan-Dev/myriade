//! The area beam (`SonicBoom`): with a target within `range_h` blocks horizontally and `range_v` vertically, and no cooldown, the entity
//! plays the charge at once, then after `sound_delay` ticks draws the beam from its chest to the target's eyes, plays the boom and
//! lands the blow (the damage and the knockback are the frame's, a socle). The act lasts `duration` ticks; the boom is the first
//! tick of the deadline of `sound_delay`, the end the deadline of `duration`.
use super::Inst;
use super::act_phase::{self, ActPhase, Phase};
use super::{params, Module};
use crate::{cow::*, goals::*, targeting::{self, Target, Who}};
use bevy_ecs::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub duration: i32,
    pub sound_delay: i32,
    /// Ticks before the next beam (the Mob sets it to the act's duration).
    pub cooldown: u64,
    /// The reach: `range_h` horizontal and `range_v` vertical, in blocks.
    pub range_h: f64,
    pub range_v: f64,
    /// Height of the chest, where the beam starts.
    pub chest: f64,
    pub charge_sound: &'static str,
    pub boom_sound: &'static str,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct Cfg(pub Params);

pub static MODULE: Module = Module {
    name: "sonic_boom",
    flags: LOOK,
    equip,
    can_use,
    start,
    stop,
    fired,
    interruptable: false,
    ..Module::DEFAULT
};

fn equip(w: &mut World, e: Entity, _: Inst, p: &(dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Cfg(*params::<Params>(p)));
    w.entity_mut(e).insert_if_new(ActPhase::default());
}

fn within(w: &World, e: Entity, who: Who, c: &Params) -> bool {
    let (Some(i), Some(p)) = (targeting::info(w, who), w.get::<Pos>(e)) else { return false };
    let (dx, dy, dz) = (i.pos.x - p.0.x, i.pos.y - p.0.y, i.pos.z - p.0.z);
    dx * dx + dz * dz < c.range_h * c.range_h && dy.abs() < c.range_v
}

fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    let now = now_of(w, e);
    let c = w.get::<Cfg>(e).unwrap().0;
    let Some(target) = w.get::<Target>(e).map(|t| t.who) else { return false };
    now >= w.get::<ActPhase>(e).unwrap().sonic_cooldown_until && targeting::can_target(w, e, target) && within(w, e, target, &c)
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
    let d = [eye[0] - src[0], eye[1] - src[1], eye[2] - src[2]];
    let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    let n = if len < 1.0e-5 { [0.0; 3] } else { d.map(|c| c / len) };
    for k in 1..len.floor() as i32 + 7 {
        let s = f64::from(k);
        let at = [src[0] + n[0] * s, src[1] + n[1] * s, src[2] + n[2] * s];
        w.resource_mut::<Events>().host.particles("sonic_boom", at, [0.0; 3], 0.0, 0);
    }
    act_phase::sound(w, e, c.boom_sound, 3.0, 1.0);
    crate::health::sonic_boom(w, e, target, n);
}

fn stop(w: &mut World, e: Entity, _: Inst) {
    w.get_mut::<ActPhase>(e).unwrap().phase = Phase::Standing;
}
