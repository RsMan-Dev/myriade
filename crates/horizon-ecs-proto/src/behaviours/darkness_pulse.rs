//! The darkness pulse: every `interval` ticks of own time, the players within `radius` blocks get the darkness effect. The effect is
//! the host's (socle `HostOut::darkness(at, radius)`, not yet written: see the notes); this module only keeps the pulse.
use super::Inst;
use super::{params, Module};
use crate::{cow::*, goals::*};
use bevy_ecs::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub radius: f64,
    pub interval: i32,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct Cfg(pub &'static Params);

pub static MODULE: Module = Module {
    name: "darkness_pulse",
    flags: 0,
    equip,
    start,
    fired,
    ..Module::DEFAULT
};

fn equip(w: &mut World, e: Entity, _: Inst, p: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Cfg(params::<Params>(p)));
}

fn start(w: &mut World, e: Entity, inst: Inst) {
    arm(w, e, inst.slot, 0);
}

fn fired(w: &mut World, e: Entity, inst: Inst) -> bool {
    let c = w.get::<Cfg>(e).unwrap().0;
    let at = w.get::<Pos>(e).unwrap().0;
    w.resource_mut::<Events>().host.darkness([at.x, at.y, at.z], c.radius);
    arm(w, e, inst.slot, c.interval);
    false
}
