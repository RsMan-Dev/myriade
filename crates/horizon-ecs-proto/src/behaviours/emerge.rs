//! The emerging act: an entity whose `emerge_pending` is set stands still for `duration` ticks. The flag is set at the birth by
//! `triggered` (the species' `equip` through `RareState`); the act is exclusive, hence `interruptable: false`.
use super::Inst;
use super::act_phase::{self, ActPhase, Phase, RareState};
use super::{params, Module};
use crate::{cow::*, goals::*};
use bevy_ecs::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub duration: i32,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct Cfg(pub &'static Params);

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

fn equip(w: &mut World, e: Entity, _: Inst, p: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Cfg(params::<Params>(p)));
    w.entity_mut(e).insert_if_new(ActPhase::default());
    w.entity_mut(e).insert_if_new(RareState::default());
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
    arm(w, e, inst.slot, c.duration);
}

fn fired(w: &mut World, e: Entity, _: Inst) -> bool {
    act_phase::deadline(w, e, &MODULE)
}

fn stop(w: &mut World, e: Entity, _: Inst) {
    w.get_mut::<ActPhase>(e).unwrap().phase = Phase::Standing;
}

/// `Warden.finalizeSpawn` for the spawn reason `Triggered` (a sculk shrieker's or a trial's call): the warden emerges at its birth, the
/// act is asked now and the agitated sound plays at once (volume 5, pitch 1). Other reasons do not emerge (the flag is not set).
pub fn triggered(w: &mut World, e: Entity) {
    if w.get::<Cfg>(e).is_none() {
        return;
    }
    if let Some(mut r) = w.get_mut::<RareState>(e) {
        r.emerge_pending = true;
    }
    act_phase::sound(w, e, "entity.warden.agitated", 5.0, 1.0);
    crate::goals::wake(w, e, &MODULE);
}
