//! `Ravager.doHurtTarget` (`ravager.rs:57`): a strike makes the body busy for `ticks` (the bite, `OCC_BITING`, which stops every
//! move) and plays the attack with the entity event for the clients. Called by the melee's strike (`Req::Call`), never by a tick.
use super::*;
use crate::{cow::Pos, goals::*};

/// The busy reason of a bite (`Ravager.counter`): read by `is_occupied` like the other reasons of `goals.rs`.
pub const OCC_BITING: u32 = 64;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub ticks: i32,
    pub event: u8,
    pub sound: &'static str,
}
impl Params {
    pub const DEFAULT: Self = Self { ticks: 10, event: 4, sound: "entity.ravager.attack" };
}
#[derive(Component, Clone, Copy, Debug)]
pub struct Bite(pub &'static Params);

pub static MODULE: Module = Module {
    name: "strike_pause",
    flags: 0,
    equip,
    requires: &[Cap::Body],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, _: Inst, p: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Bite(params::<Params>(p)));
    crate::hostile::ensure_attack(w, e);
    w.get_mut::<crate::hostile::Attack>(e).unwrap().on_blow = Some(on_strike);
}
/// A strike, landed or not (`attackTick` is set before `hurtServer`): busy until the wheel's deadline releases the body.
pub fn on_strike(w: &mut World, e: Entity) {
    let Some(b) = w.get::<Bite>(e).map(|b| b.0) else { return };
    occupy(w, e, OCC_BITING);
    super::hunt_speed::refresh(w, e);
    let at = w.get::<Pos>(e).map_or([0.0; 3], |p| [p.0.x, p.0.y, p.0.z]);
    let host = &mut w.resource_mut::<Events>().host;
    host.sound(b.sound, at, 1.0, 1.0);
    host.entity_event(e, b.event);
    crate::cow::after(w, e, b.ticks, release_bite);
}
fn release_bite(w: &mut World, e: Entity) {
    release(w, e, OCC_BITING);
    super::hunt_speed::refresh(w, e);
}
