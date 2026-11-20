//! The sniffing act: with no sniff cooldown, no disturbance and no target, a creature it could attack nearby is sniffed for `duration`
//! ticks, and the cooldown restarts for `cooldown` ticks. Its cooldown is a deadline (`ActPhase.sniff_cooldown_until`), not a count.
use super::Inst;
use super::act_phase::{self, ActPhase, Phase, RareState};
use super::{params, Module};
use crate::{cow::*, goals::*, targeting::Target};
use bevy_ecs::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub duration: i32,
    /// The cooldown after a sniff, in ticks (a fixed value: the random draw of the source is not ported).
    pub cooldown: u64,
    pub sound: &'static str,
    /// The range of the creatures that keep it from sniffing (its follow range).
    pub radius: f64,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct Cfg(pub Params);

pub static MODULE: Module = Module {
    name: "sniff",
    flags: MOVE | LOOK,
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
    w.entity_mut(e).insert_if_new(RareState::default());
}

fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    let now = now_of(w, e);
    now >= w.get::<ActPhase>(e).unwrap().sniff_cooldown_until
        && w.get::<RareState>(e).unwrap().disturbance.is_none()
        && w.get::<Target>(e).is_none()
        && !w.get::<Stroll>(e).is_some_and(|s| s.walking)
        && crate::targeting::attackable_near(w, e, w.get::<Cfg>(e).unwrap().0.radius).is_none()
}

fn start(w: &mut World, e: Entity, inst: Inst) {
    let c = w.get::<Cfg>(e).unwrap().0;
    let now = now_of(w, e);
    if let Some(mut s) = w.get_mut::<Stroll>(e) {
        s.walking = false;
    }
    let mut st = w.get_mut::<ActPhase>(e).unwrap();
    st.phase = Phase::Sniffing;
    st.until = now + c.duration as u64;
    st.sniff_cooldown_until = now + c.cooldown;
    drop(st);
    act_phase::sound(w, e, c.sound, 5.0, 1.0);
    arm(w, e, inst.slot, c.duration);
}

fn fired(w: &mut World, e: Entity, _: Inst) -> bool {
    act_phase::deadline(w, e, &MODULE)
}

fn stop(w: &mut World, e: Entity, _: Inst) {
    w.get_mut::<ActPhase>(e).unwrap().phase = Phase::Standing;
}
