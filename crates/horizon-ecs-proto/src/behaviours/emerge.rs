//! The emerging act: an entity whose `emerge_pending` is set plays the emerge sound and stands still for `duration` ticks. The flag is
//! set by the species' `equip` through `RareState`; the act is exclusive, hence `interruptable: false`.
use super::Inst;
use super::act_phase::{self, ActPhase, Phase, RareState};
use super::{params, Module};
use crate::{cow::*, goals::*};
use bevy_ecs::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub duration: i32,
    pub sound: &'static str,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct Cfg(pub Params);

pub static MODULE: Module = Module {
    name: "emerge",
    flags: MOVE | LOOK,
    equip,
    can_use,
    start,
    stop,
    fired,
    interruptable: false,
    ..Module::DEFAULT
};

fn equip(w: &mut World, e: Entity, inst: Inst, p: &(dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Cfg(*params::<Params>(p)));
    w.entity_mut(e).insert_if_new(ActPhase::default());
    w.entity_mut(e).insert_if_new(RareState { emerge_pending: true, ..RareState::default() });
    // Nobody else asks for it: the animal climbs out of the ground at its first tick.
    crate::goals::arm(w, e, inst.slot, 1);
}

fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    w.get::<RareState>(e).unwrap().emerge_pending && !w.get::<Stroll>(e).is_some_and(|s| s.walking)
}

fn start(w: &mut World, e: Entity, inst: Inst) {
    let c = w.get::<Cfg>(e).unwrap().0;
    let now = now_of(w, e);
    w.get_mut::<RareState>(e).unwrap().emerge_pending = false;
    let mut st = w.get_mut::<ActPhase>(e).unwrap();
    st.phase = Phase::Emerging;
    st.until = now + c.duration as u64;
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
