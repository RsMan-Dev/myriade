//! `homing_shot`: a goal that, while its target stands within `reach_sq` and the difficulty is not peaceful, opens the body's lid to `open`,
//! turns to the target's eyes (`aim_at`, `turn` degrees a tick) and fires a `homing_projectile` every `wait_base + wait_step x nextInt(wait_count)`
//! ticks, from its box's middle, with a sound. The lid shuts when the goal stops. Flags MOVE | LOOK.
use crate::cow::Pos;
use super::*;
use crate::behaviours::{attached_face::{Attached, Sounds, set_peek}, fox_state::aim_at, homing_projectile};
use crate::goals::*;
use crate::targeting::{self, Target};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub reach_sq: f64,
    pub open: i32,
    pub turn: f64,
    pub wait_base: i32,
    pub wait_step: i32,
    pub wait_count: i32,
    pub shot: &'static homing_projectile::Config,
    pub sound: &'static str,
    pub volume: f32,
    pub pitch_spread: f32,
    pub sounds: Sounds,
    pub covered: bool,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct Volley {
    pub wait: i32,
    pub cfg: Params,
    pub slot: u8,
}
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Firing;

pub static MODULE: Module = Module {
    name: "homing_shot",
    flags: MOVE | LOOK,
    resources,
    equip,
    can_use,
    start,
    stop,
    tick: Some(tick),
    requires: &[Cap::Body, Cap::Goals, Cap::Head, Cap::Targets],
    ..Module::DEFAULT
};
/// The target service tells the goal when its creature gets a target: it asks `can_use` then (nothing else wakes it).
fn resources(w: &mut World) {
    w.resource_mut::<targeting::TargetWatchers>().0.push(|w, e| {
        if let Some(slot) = w.get::<Volley>(e).map(|v| v.slot) {
            w.resource_mut::<Requests>().0.push(Req::Start(e, slot));
        }
    });
}
fn equip(w: &mut World, e: Entity, inst: Inst, p: &(dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Volley { wait: 0, cfg: *params::<Params>(p), slot: inst.slot });
}
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    crate::effects::difficulty(w) != 0 && w.get::<Target>(e).is_some_and(|t| targeting::info(w, t.who).is_some_and(|i| !i.exempt))
}
fn start(w: &mut World, e: Entity, _: Inst) {
    let cfg = {
        let mut v = w.get_mut::<Volley>(e).unwrap();
        v.wait = 20;
        v.cfg
    };
    w.entity_mut(e).insert(Firing);
    set_peek(w, e, cfg.open, &cfg.sounds, cfg.covered);
}
fn stop(w: &mut World, e: Entity, _: Inst) {
    let cfg = w.get::<Volley>(e).unwrap().cfg;
    w.entity_mut(e).remove::<Firing>();
    set_peek(w, e, 0, &cfg.sounds, cfg.covered);
}
fn tick(w: &mut World) {
    crate::goals::tick_state(w, visit);
}
fn visit(q: Query<Entity, With<Firing>>, mut cmd: Commands) {
    for e in &q {
        cmd.queue(move |w: &mut World| step(w, e));
    }
}
/// The tick, in the vanilla order: peaceful stops it; the count goes down; no target ends the step; the aim; a target out of reach drops it;
/// the wait not up ends the step; else the new wait, the projectile, the sound. The draws come in that order (wait, seed, pitch).
fn step(w: &mut World, e: Entity) {
    if crate::effects::difficulty(w) == 0 {
        return;
    }
    let (wait, cfg) = {
        let mut v = w.get_mut::<Volley>(e).unwrap();
        v.wait -= 1;
        (v.wait, v.cfg)
    };
    let Some(t) = w.get::<Target>(e).copied() else { return };
    let Some(i) = targeting::info(w, t.who) else { return };
    aim_at(w, e, [i.pos.x, i.eye, i.pos.z], Some(cfg.turn as f32));
    let from = w.get::<Pos>(e).map(|p| p.0).unwrap_or(i.pos);
    if d2(from, i.pos) >= cfg.reach_sq {
        targeting::clear_target(w, e);
        return;
    }
    if wait > 0 {
        return;
    }
    let next = cfg.wait_base + cfg.wait_step * with_rng(w, e, |r| r.next_int(cfg.wait_count));
    w.get_mut::<Volley>(e).unwrap().wait = next;
    let seed = with_rng(w, e, |r| (u64::from(r.next_u32()) << 32 | u64::from(r.next_u32())) | 1);
    let axis = homing_projectile::axis_of(w.get::<Attached>(e).map_or(0, |a| a.face));
    homing_projectile::spawn(w, e, t.who, axis, seed, cfg.shot);
    let pitch = with_rng(w, e, |r| (r.next_f64() - r.next_f64()) as f32 * cfg.pitch_spread + 1.0);
    let at = w.get::<Pos>(e).map(|p| [p.0.x, p.0.y, p.0.z]).unwrap_or([0.0; 3]);
    w.resource_mut::<Events>().host.sound(cfg.sound, at, cfg.volume, pitch);
}
