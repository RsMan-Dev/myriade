//! `IronGolem.doHurtTarget`: the attack event goes to the clients at every strike (`on_blow`); a strike that lands tosses the target up,
//! `lift * (1 - clamp(knockback resistance, 0, 1))` added to its vertical velocity (`on_hit`). Never by a tick.
//! A player target is tossed by the host (`push_player`), with no resistance known here.
use super::*;
use crate::cow::{Resist, Vel};
use crate::goals::Events;
use crate::targeting::Who;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub lift: f64,
    pub event: u8,
}
impl Params {
    pub const DEFAULT: Self = Self { lift: 0.4000000059604645, event: 4 };
}
#[derive(Component, Clone, Copy, Debug)]
pub struct Toss(pub &'static Params);

pub static MODULE: Module = Module { name: "target_toss", flags: 0, equip, requires: &[Cap::Body], ..Module::DEFAULT };
fn equip(w: &mut World, e: Entity, _: Inst, p: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Toss(params::<Params>(p)));
    crate::hostile::ensure_attack(w, e);
    let mut a = w.get_mut::<crate::hostile::Attack>(e).unwrap();
    a.on_blow = Some(on_blow);
    a.on_hit = Some(on_strike);
    // `getAttackDamage() / 2 + random.nextInt((int) damage)`.
    a.spread = true;
}
/// Every strike: the attack event.
fn on_blow(w: &mut World, e: Entity) {
    let Some(t) = w.get::<Toss>(e).map(|t| t.0) else { return };
    w.resource_mut::<Events>().host.entity_event(e, t.event);
}
/// A strike that landed: the toss of the target.
pub fn on_strike(w: &mut World, e: Entity, who: Who, _dealt: f32) {
    let Some(t) = w.get::<Toss>(e).map(|t| t.0) else { return };
    let lift = |r: f64| t.lift * (1.0 - r.clamp(0.0, 1.0));
    match who {
        Who::Player(id) => w.resource_mut::<Events>().host.push_player(id, [0.0, lift(0.0), 0.0]),
        Who::Entity(o) => {
            let up = lift(w.get::<Resist>(o).map_or(0.0, |r| r.0));
            if let Some(mut v) = w.get_mut::<Vel>(o) {
                v.0.y += up;
            }
        }
    }
}
