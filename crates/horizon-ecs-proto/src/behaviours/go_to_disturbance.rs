//! Walking to the disturbance: with no target and no walk, the entity walks to the place of the last vibration it kept (`RareState`,
//! until its expiry), and stops within 2 blocks. Woken by `vibration_listen::receive`.
use super::Inst;
use super::act_phase::RareState;
use super::{params, Module};
use crate::{cow::*, goals::*, targeting::Target};
use bevy_ecs::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub speed: f64,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct Cfg(pub Params);

use crate::rules::go_to_disturbance::{spot_of, still_away};

pub static MODULE: Module = Module {
    name: "go_to_disturbance",
    flags: MOVE | LOOK,
    ends_with_path: true,
    equip,
    can_use,
    start,
    ..Module::DEFAULT
};

fn equip(w: &mut World, e: Entity, _: Inst, p: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Cfg(*params::<Params>(p)));
    w.entity_mut(e).insert_if_new(RareState::default());
}

fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    let now = now_of(w, e);
    let r = *w.get::<RareState>(e).unwrap();
    let Some(p) = r.disturbance.filter(|_| now < r.disturbance_until) else { return false };
    if w.get::<Target>(e).is_some() || w.get::<Stroll>(e).is_some_and(|s| s.walking) {
        return false;
    }
    let at = w.get::<Pos>(e).unwrap().0;
    still_away(p, [at.x, at.y, at.z])
}

fn start(w: &mut World, e: Entity, _: Inst) {
    let speed = w.get::<Cfg>(e).unwrap().0.speed;
    let Some(p) = w.get::<RareState>(e).unwrap().disturbance else { return };
    let spot = spot_of(p);
    crate::brain::look_at(w, e, spot);
    w.get_mut::<Nav>(e).unwrap().speed = speed;
    let mut s = w.get_mut::<Stroll>(e).unwrap();
    (s.tx, s.ty, s.tz) = (spot[0], spot[1].floor() as i32, spot[2]);
    s.walking = true;
    drop(s);
    w.entity_mut(e).insert(NeedsPath);
}
