//! Lookout (`Fox.PerchAndSearchGoal`): now and then the animal sits (SITTING bit) and looks in two to four random directions, one
//! after the other, each for a random number of ticks; the goal ends with the last look. The directions are drawn at the start.
//! DOUBT: vanilla draws each look when the previous one ends; here they are drawn together (same spread, other order).
use super::fox_state as pose;
use super::*;
use crate::rules::lookout::{ask_period, look_count, look_dir, look_ticks};
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
    /// The next ask's deadline is in the wheel.
    pub armed: bool,
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
    blocked: Some(blocked),
    equip,
    arm,
    fired,
    can_use,
    start,
    stop,
    tick: Some(tick),
    requires: &[Cap::Body, Cap::Head],
    ..Module::DEFAULT
};

fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let p = params::<Params>(p);
    w.entity_mut(e).insert(Lookout { p, slot: inst.slot, armed: false });
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
        arm(w, e, inst);
        return false;
    }
    true
}

/// Arms the next ask: the selector asks every other tick and the goal passes one ask in `1 / chance` (a geometric wait, nothing counted).
fn arm(w: &mut World, e: Entity, inst: Inst) {
    let Some(g) = w.get::<Lookout>(e).copied() else { return };
    if g.armed || g.slot != inst.slot {
        return;
    }
    let period = ask_period(g.p.chance);
    let wait = with_rng(w, e, |r| 2 * geometric(r, period));
    crate::goals::arm(w, e, inst.slot, wait);
    w.get_mut::<Lookout>(e).unwrap().armed = true;
}
fn blocked(w: &mut World, e: Entity, inst: Inst) {
    if let Some(mut g) = w.get_mut::<Lookout>(e) {
        g.armed = false;
    }
    arm(w, e, inst);
}
fn fired(w: &mut World, e: Entity, _: Inst) -> bool {
    match w.get_mut::<Lookout>(e) {
        Some(mut g) => {
            g.armed = false;
            true
        }
        None => false,
    }
}

fn start(w: &mut World, e: Entity, inst: Inst) {
    let Some(g) = w.get::<Lookout>(e).copied() else { return };
    if g.slot != inst.slot {
        return;
    }
    let mut looks = with_rng(w, e, |r| {
        let n = look_count(r.next_int(3));
        (0..n)
            .map(|_| {
                let dir = look_dir(std::f64::consts::TAU * r.next_f64());
                (dir, look_ticks(r.next_int(20)))
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
        blocked(w, e, inst);
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
