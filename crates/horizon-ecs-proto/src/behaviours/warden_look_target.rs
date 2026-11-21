//! `SetWardenLookTarget` du Warden (`warden_brain.rs:738-760`, core prio 1) : sans cible d'attaque, il regarde le bloc de sa
//! dernière perturbation tant qu'elle dure (`RareState.disturbance`). Avec une cible d'attaque, le regard suit la cible.
use super::act_phase::RareState;
use super::Inst;
use super::Module;
use crate::rules::warden_look_target::{block_centre, eye_point, in_follow_range, still_disturbed};
use crate::{cow::*, goals::*, targeting::Target};
use bevy_ecs::prelude::*;

/// The goal follows the attack target with its head (`SetEntityLookTarget`, Fight, `warden_brain.rs:995`: the target within 24 blocks).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
struct Watching(u8);

pub static MODULE: Module = Module { name: "warden_look_target", flags: LOOK, can_use, start, stop, tick: Some(tick), resources, ..Module::DEFAULT };

/// A new target asks the goal 3 ticks later (measured on the Mob: its head starts to turn 3 ticks after the hit; no poll).
fn resources(w: &mut World) {
    w.resource_mut::<crate::targeting::TargetWatchers>().0.push(|w, e| {
        if w.get::<GoalSet>(e).is_some_and(|g| g.has(&MODULE)) {
            arm_module(w, e, &MODULE, 3);
        }
    });
}

fn watched(w: &World, e: Entity) -> Option<[f64; 3]> {
    let who = w.get::<Target>(e)?.who;
    let (me, i) = (w.get::<Pos>(e)?.0, crate::targeting::info(w, who)?);
    in_follow_range(crate::goals::d2(me, i.pos)).then_some(eye_point(i.pos, i.eye))
}
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<Watching>();
}
fn tick(w: &mut World) {
    let es: Vec<(Entity, u8)> = w.query::<(Entity, &Watching)>().iter(w).map(|(e, s)| (e, s.0)).collect();
    for (e, slot) in es {
        match watched(w, e) {
            Some(at) => crate::brain::look_at(w, e, at),
            None => w.resource_mut::<Requests>().0.push(Req::End(e, slot)),
        }
    }
}

fn disturbance(w: &World, e: Entity) -> Option<[i32; 3]> {
    let now = now_of(w, e);
    let r = w.get::<RareState>(e)?;
    r.disturbance.filter(|_| still_disturbed(now, r.disturbance_until))
}

fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    if w.get::<Target>(e).is_some() {
        return watched(w, e).is_some();
    }
    disturbance(w, e).is_some()
}

fn start(w: &mut World, e: Entity, inst: Inst) {
    if let Some(at) = watched(w, e) {
        w.entity_mut(e).insert(Watching(inst.slot));
        crate::brain::look_at(w, e, at);
        return;
    }
    let Some(p) = disturbance(w, e) else { return };
    crate::brain::look_at(w, e, block_centre(p));
}
