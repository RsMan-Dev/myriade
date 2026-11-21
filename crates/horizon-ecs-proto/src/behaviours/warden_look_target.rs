//! `SetWardenLookTarget` du Warden (`warden_brain.rs:738-760`, core prio 1) : sans cible d'attaque, il regarde le bloc de sa
//! dernière perturbation tant qu'elle dure (`RareState.disturbance`). Avec une cible d'attaque, le regard suit la cible.
use super::act_phase::RareState;
use super::Inst;
use super::Module;
use crate::{cow::*, goals::*, targeting::Target};
use bevy_ecs::prelude::*;

pub static MODULE: Module = Module { name: "warden_look_target", flags: LOOK, can_use, start, ..Module::DEFAULT };

fn disturbance(w: &World, e: Entity) -> Option<[i32; 3]> {
    let now = now_of(w, e);
    let r = w.get::<RareState>(e)?;
    r.disturbance.filter(|_| now < r.disturbance_until)
}

fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    w.get::<Target>(e).is_none() && disturbance(w, e).is_some()
}

fn start(w: &mut World, e: Entity, _: Inst) {
    let Some(p) = disturbance(w, e) else { return };
    crate::brain::look_at(w, e, [p[0] as f64 + 0.5, p[1] as f64 + 0.5, p[2] as f64 + 0.5]);
}
