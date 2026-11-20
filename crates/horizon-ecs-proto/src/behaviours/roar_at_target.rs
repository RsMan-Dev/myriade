//! The roar at the target (`SetRoarTarget` and `Roar`): an angry entity with a suspect it can target, and no target of its own,
//! roars at it for `duration` ticks, looks at it, and angers it by `anger` more. (The ravager's roar is another module.)
use super::Inst;
use super::act_phase::{self, ActPhase, Phase, RareState};
use super::{params, suspect_anger::{self, Level}, Module};
use crate::{cow::*, goals::*, targeting::{self, Target}};
use bevy_ecs::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub duration: i32,
    pub anger: u16,
    pub sound: &'static str,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct Cfg(pub Params);

pub static MODULE: Module = Module {
    name: "roar_at_target",
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
    w.entity_mut(e).insert(Cfg(*params::<Params>(p)));
    w.entity_mut(e).insert_if_new(ActPhase::default());
    w.entity_mut(e).insert_if_new(RareState::default());
}

fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    if suspect_anger::level(w, e) != Level::Angry || w.get::<Target>(e).is_some() {
        return false;
    }
    let Some(&(who, _)) = suspect_anger::live(w, e).first() else { return false };
    w.get_mut::<RareState>(e).unwrap().roar_target = Some(who);
    true
}

fn start(w: &mut World, e: Entity, inst: Inst) {
    let c = w.get::<Cfg>(e).unwrap().0;
    let now = now_of(w, e);
    let Some(target) = w.get::<RareState>(e).unwrap().roar_target else { return };
    let mut r = w.get_mut::<RareState>(e).unwrap();
    r.roar_target = None;
    drop(r);
    let mut st = w.get_mut::<ActPhase>(e).unwrap();
    st.phase = Phase::Roaring;
    st.until = now + c.duration as u64;
    drop(st);
    if let Some(i) = targeting::info(w, target) {
        crate::brain::look_at(w, e, [i.pos.x, i.pos.y, i.pos.z]);
    }
    if let Some(mut s) = w.get_mut::<Stroll>(e) {
        s.walking = false;
    }
    act_phase::sound(w, e, c.sound, 3.0, 1.0);
    suspect_anger::raise(w, e, target, c.anger);
    arm(w, e, inst.slot, c.duration);
}

fn fired(w: &mut World, e: Entity, _: Inst) -> bool {
    act_phase::deadline(w, e, &MODULE)
}

fn stop(w: &mut World, e: Entity, _: Inst) {
    w.get_mut::<ActPhase>(e).unwrap().phase = Phase::Standing;
}
