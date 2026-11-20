//! The burrowing act: an entity with no target and no walk digs for `duration` ticks once its dig cooldown is over, then leaves the
//! world (`Withdrawn`, which the frame takes out of its queries). In a liquid it does not dig: it leaves at once, and its sound is
//! the liquid one. A rider that was forced aboard gets off first (`mount::dismount`). The cooldown is set by `suspect_anger::raise`.
use super::Inst;
use super::act_phase::{self, ActPhase, Phase};
use super::{params, Module};
use crate::{cow::*, goals::*, mount, targeting::Target};
use bevy_ecs::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub duration: i32,
    /// Ticks without digging after the target is lost (`WardenAi.onTargetInvalid`).
    pub cooldown: u64,
    pub sound_ground: &'static str,
    pub sound_liquid: &'static str,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct Cfg(pub Params);

/// The entity is out of the world: the frame keeps it out of the queries until something takes the marker away.
#[derive(Component, Clone, Copy, Debug)]
pub struct Withdrawn;

pub static MODULE: Module = Module {
    name: "dig",
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
}

fn can_use(w: &mut World, e: Entity, inst: Inst) -> bool {
    let now = now_of(w, e);
    let until = w.get::<ActPhase>(e).unwrap().dig_cooldown_until;
    if now < until {
        arm(w, e, inst.slot, (until - now) as i32);
        return false;
    }
    let grounded = w.get::<Contact>(e).is_some_and(|c| c.on_ground);
    let fluid = w.get::<FluidState>(e).is_some_and(|f| f.in_water || f.in_lava);
    let ok = (grounded || fluid) && w.get::<Target>(e).is_none() && !w.get::<Stroll>(e).is_some_and(|s| s.walking);
    if !ok {
        // Asked again soon: nothing else wakes a goal that is refused for a transient reason.
        arm(w, e, inst.slot, 20);
    }
    ok
}

fn start(w: &mut World, e: Entity, inst: Inst) {
    let c = w.get::<Cfg>(e).unwrap().0;
    let now = now_of(w, e);
    mount::dismount(w, e);
    let on_ground = w.get::<Contact>(e).is_some_and(|c| c.on_ground);
    if !on_ground {
        act_phase::sound(w, e, c.sound_liquid, 5.0, 1.0);
        w.entity_mut(e).insert(Withdrawn);
        return;
    }
    let mut st = w.get_mut::<ActPhase>(e).unwrap();
    st.phase = Phase::Burrowing;
    st.until = now + c.duration as u64;
    drop(st);
    act_phase::sound(w, e, c.sound_ground, 5.0, 1.0);
    arm(w, e, inst.slot, c.duration);
}

fn fired(w: &mut World, e: Entity, _: Inst) -> bool {
    act_phase::deadline(w, e, &MODULE)
}

fn stop(w: &mut World, e: Entity, _: Inst) {
    let now = now_of(w, e);
    let st = *w.get::<ActPhase>(e).unwrap();
    if st.phase == Phase::Burrowing && now >= st.until {
        w.entity_mut(e).insert(Withdrawn);
    }
    w.get_mut::<ActPhase>(e).unwrap().phase = Phase::Standing;
}

/// The target is gone: the dig cooldown starts again (it is also kept up while the fight lasts, by the anger raised in it).
pub fn target_lost(w: &mut World, e: Entity) {
    let Some(c) = w.get::<Cfg>(e).map(|c| c.0.cooldown) else { return };
    let now = now_of(w, e);
    if let Some(mut st) = w.get_mut::<ActPhase>(e) {
        st.dig_cooldown_until = now + c;
        arm_module(w, e, &MODULE, c as i32);
    }
}
