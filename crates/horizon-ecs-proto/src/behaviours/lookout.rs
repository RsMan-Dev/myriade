//! Lookout (`Fox.PerchAndSearchGoal`): now and then the animal sits (SITTING bit) and looks in two to four random directions, one
//! after the other, each for a random number of ticks; the goal ends with the last look. The directions are drawn at the start.
//! DOUBT: vanilla draws each look when the previous one ends; here they are drawn together (same spread, other order).
use super::fox_state as pose;
use super::*;
use crate::{cow::*, goals::*};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// The chance per ask that the animal starts a lookout.
    pub chance: f64,
    /// Bits that must be clear to start (sleeping, pouncing, crouching) and the bit set while sitting.
    pub blocked: u8,
    pub sit: u8,
    /// `Fox.alertable`: true when a creature or a player near refuses the lookout (the species' own check).
    pub alarm: Option<fn(&World, Entity) -> bool>,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct Lookout {
    pub p: &'static Params,
    pub slot: u8,
}

/// The lookout in progress: the looks still to do (direction, ticks), the ticks left of the current one and its direction.
#[derive(Component, Clone, Debug)]
#[component(storage = "SparseSet")]
pub struct Looking {
    pub slot: u8,
    pub looks: Vec<([f64; 2], i32)>,
    pub look_time: i32,
    pub rel: [f64; 2],
}

pub static MODULE: Module = Module {
    name: "lookout",
    flags: MOVE | LOOK,
    equip,
    can_use,
    start,
    stop,
    tick: Some(tick),
    requires: &[Cap::Body, Cap::Head],
    ..Module::DEFAULT
};

fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let p = params::<Params>(p);
    w.entity_mut(e).insert(Lookout { p, slot: inst.slot });
}

/// The module's tick, called only while a creature that carries it lives (no registered system).
fn tick(w: &mut World) {
    crate::goals::tick_state(w, look_run);
}

fn can_use(w: &mut World, e: Entity, inst: Inst) -> bool {
    let Some(g) = w.get::<Lookout>(e).copied() else { return false };
    let alarmed = g.p.alarm.is_some_and(|f| f(w, e));
    if g.slot != inst.slot
        || w.get::<LastHurtBy>(e).is_some()
        || w.get::<crate::targeting::Target>(e).is_some()
        || pose::has(w, e, g.p.blocked)
        || w.get::<Nav>(e).is_some_and(|n| n.path.is_some())
        || alarmed
    {
        return false;
    }
    with_rng(w, e, |r| r.next_f64()) < f64::from(g.p.chance)
}

fn start(w: &mut World, e: Entity, inst: Inst) {
    let Some(g) = w.get::<Lookout>(e).copied() else { return };
    if g.slot != inst.slot {
        return;
    }
    let mut looks = with_rng(w, e, |r| {
        let n = 2 + r.next_int(3);
        (0..n)
            .map(|_| {
                let a = std::f64::consts::TAU * r.next_f64();
                ([a.cos(), a.sin()], (80 + r.next_int(20) + 1) / 2)
            })
            .collect::<Vec<([f64; 2], i32)>>()
    });
    let first = looks.remove(0);
    pose::set(w, e, g.p.sit, true);
    w.entity_mut(e).insert(Looking { slot: g.slot, looks, look_time: first.1, rel: first.0 });
}

fn stop(w: &mut World, e: Entity, inst: Inst) {
    let Some(g) = w.get::<Lookout>(e).copied() else { return };
    if g.slot == inst.slot {
        pose::set(w, e, g.p.sit, false);
        w.entity_mut(e).remove::<Looking>();
    }
}

/// `PerchAndSearchGoal.tick`: each look ends when its ticks run out and the next one starts; the last one ends the goal.
fn look_run(mut cmd: Commands, mut q: Query<(Aim, &mut Looking)>, mut reqs: ResMut<Requests>) {
    for ((e, pos, mut look, looking, _, _), mut s) in &mut q {
        if s.look_time > 0 {
            s.look_time -= 1;
            let at = [pos.0.x + s.rel[0], pos.0.y + look.eye, pos.0.z + s.rel[1]];
            aim(&mut cmd, e, &mut look, looking, at, None);
        } else if s.looks.is_empty() {
            reqs.0.push(Req::End(e, s.slot));
        } else {
            let (rel, ticks) = s.looks.remove(0);
            s.rel = rel;
            s.look_time = ticks;
        }
    }
}
