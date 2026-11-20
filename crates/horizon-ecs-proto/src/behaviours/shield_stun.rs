//! `Ravager.blockedByItem` (`ravager.rs:73`): a blow that a raised shield blocks stuns the ravager with `chance`, for `ticks`
//! (busy with `OCC_STUNNED`, and blind: `sight` reads it). The stun's end starts the roar (`ravager_roar::start`). When it answers false,
//! the caller flings the defender, as vanilla does.
use super::*;
use crate::goals::*;

/// The busy reason of a stun (`Ravager.counter2`).
pub const OCC_STUNNED: u32 = 128;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub chance: f64,
    pub ticks: i32,
}
impl Params {
    pub const DEFAULT: Self = Self { chance: 0.5, ticks: 40 };
}
#[derive(Component, Clone, Copy, Debug)]
pub struct Stun(pub Params);

pub static MODULE: Module = Module {
    name: "shield_stun",
    flags: 0,
    equip,
    requires: &[Cap::Body],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, _: Inst, p: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Stun(*params::<Params>(p)));
}
/// `blockedByItem`: not while roaring (no draw then), else one draw; true when the blow stuns.
pub fn on_blocked(w: &mut World, e: Entity) -> bool {
    let Some(s) = w.get::<Stun>(e).map(|s| s.0) else { return false };
    if is_occupied(w, e, super::ravager_roar::OCC_ROARING) || !chance(w, e, f64::from(s.chance)) {
        return false;
    }
    occupy(w, e, OCC_STUNNED);
    super::hunt_speed::refresh(w, e);
    let at = w.get::<crate::cow::Pos>(e).map_or([0.0; 3], |p| [p.0.x, p.0.y, p.0.z]);
    let host = &mut w.resource_mut::<Events>().host;
    host.entity_event(e, 39);
    host.sound("entity.ravager.stunned", at, 1.0, 1.0);
    // The vanilla stars (one tick in six over the stun) come as one burst of their count, at the stun: no tick reads them.
    host.particles("entity_effect", at, [0.3, 0.5, 0.3], 0.0, s.ticks as u32 / 6);
    crate::cow::after(w, e, s.ticks, end_stun);
    true
}
fn end_stun(w: &mut World, e: Entity) {
    release(w, e, OCC_STUNNED);
    super::hunt_speed::refresh(w, e);
    super::ravager_roar::start(w, e);
}
