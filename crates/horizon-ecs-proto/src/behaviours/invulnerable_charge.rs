//! The wither's charge (`WitherBoss.makeInvulnerable`, `customServerAiStep` while the counter runs): `ticks` of standing still
//! (MOVE | JUMP | LOOK taken), then the blast from its eyes (radius 7, no fire, itself as source) and the roar where it stands.
//! The end is one wheel deadline (`Due::Goal`), no tick.
use super::*;
use crate::cow::*;
use crate::goals::*;
use crate::projectile::Blast;
use crate::targeting::{Eye, Who};
use horizon_core::geometry::Vec3;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub ticks: i32,
    pub radius: f32,
    pub sound: &'static str,
}
impl Params {
    pub const DEFAULT: Self = Self { ticks: 220, radius: 7.0, sound: "entity.wither.spawn" };
}
#[derive(Component, Clone, Copy, Debug)]
pub struct Charge {
    until: u64,
    slot: u8,
    p: Params,
    done: bool,
}
pub static MODULE: Module = Module {
    name: "invulnerable_charge",
    flags: MOVE | JUMP | LOOK,
    equip,
    fired,
    can_use,
    start,
    requires: &[Cap::Body],
    ..Module::DEFAULT
};
/// `horizon:is_charging`: the animal is still charging.
pub fn charging(w: &World, e: Entity) -> bool {
    w.get::<Charge>(e).is_some_and(|c| !c.done)
}
/// The tick the charge ends, while it holds.
pub fn ends_at(w: &World, e: Entity) -> Option<u64> {
    w.get::<Charge>(e).filter(|c| !c.done).map(|c| c.until)
}
fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let p = *params::<Params>(p);
    let until = now_of(w, e) + p.ticks.max(0) as u64;
    w.entity_mut(e).insert(Charge { until, slot: inst.slot, p, done: false });
    arm_at(w, e, until, Due::Goal(inst.slot));
    crate::goals::arm(w, e, inst.slot, 0);
}
fn fired(w: &mut World, e: Entity, _: Inst) -> bool {
    w.get::<Charge>(e).is_some_and(|c| !c.done && now_of(w, e) >= c.until)
}
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    charging(w, e)
}
/// Runs while the charge holds; the blast and the end come at the deadline.
fn start(w: &mut World, e: Entity, inst: Inst) {
    let Some(c) = w.get::<Charge>(e).copied() else { return };
    if c.done || now_of(w, e) < c.until {
        return;
    }
    w.get_mut::<Charge>(e).unwrap().done = true;
    let pos = w.get::<Pos>(e).map_or(Vec3::ZERO, |p| p.0);
    let eye = w.get::<Eye>(e).map_or(1.6, |l| l.0);
    let mut ev = w.resource_mut::<Events>();
    ev.exploded.push(Blast {
        at: Vec3::new(pos.x, pos.y + eye, pos.z),
        radius: c.p.radius,
        fire: false,
        owner: Some(Who::Entity(e)),
        source: Some(e),
    });
    ev.host.sound(c.p.sound, [pos.x, pos.y, pos.z], 1.0, 1.0);
    w.resource_mut::<Requests>().0.push(Req::End(e, inst.slot));
}
