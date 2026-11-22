//! `Ravager.roar` (`ravager.rs:244`): started by the end of a stun (`shield_stun`). Busy for 20 ticks (`OCC_ROARING`); at 10 ticks it
//! hits the players and the creatures in the box inflated by 4 (6 damage, flung by 4; the ravagers and the illagers spared) and puffs
//! smoke (40 particles). Two timers on the wheel, no tick.
use super::*;
use crate::{cow::Pos, goals::*};
use crate::rules::ravager_roar::fling;

/// The busy reason of a roar (`Ravager.anim_ticks`).
pub const OCC_ROARING: u32 = 256;
/// The roar's hit comes at this tick, the end at the next one (`anim_ticks` 20, the hit at 10).
const HIT_AT: i32 = 10;
const END_AT: i32 = 20;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub range: f64,
    pub damage: f32,
    pub fling: f64,
    pub sound: &'static str,
    /// The species the roar spares.
    pub spared: &'static [&'static str],
}
#[derive(Component, Clone, Copy, Debug)]
pub struct Roar(pub &'static Params);

pub static MODULE: Module = Module {
    name: "ravager_roar",
    flags: 0,
    equip,
    requires: &[Cap::Body],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, _: Inst, p: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Roar(params::<Params>(p)));
}
/// The roar starts: busy, the sound, and the two deadlines.
pub fn start(w: &mut World, e: Entity) {
    let Some(sound) = w.get::<Roar>(e).map(|r| r.0.sound) else { return };
    occupy(w, e, OCC_ROARING);
    super::hunt_speed::refresh(w, e);
    let at = w.get::<Pos>(e).map_or([0.0; 3], |p| [p.0.x, p.0.y, p.0.z]);
    w.resource_mut::<Events>().host.sound(sound, at, 1.0, 1.0);
    crate::cow::after(w, e, HIT_AT, hit);
    crate::cow::after(w, e, END_AT, end);
}
fn end(w: &mut World, e: Entity) {
    release(w, e, OCC_ROARING);
    super::hunt_speed::refresh(w, e);
}
/// The blow of the roar: the players and the creatures in the box, then the puff.
fn hit(w: &mut World, e: Entity) {
    let Some(r) = w.get::<Roar>(e).map(|r| r.0) else { return };
    let Some(at) = w.get::<Pos>(e).map(|p| p.0) else { return };
    let from = (at.x, at.z);
    let area = crate::area::reach_box(w, e, r.range);
    use crate::area::*;
    for p in players_in(w, area) {
        hurt_player(w, p, r.damage, e);
        push_player(w, p, fling(from, player_xz(w, p), r.fling));
    }
    for c in creatures_in(w, area, e) {
        if r.spared.contains(&species_name(w, c)) {
            continue;
        }
        // `ArmorStand.hurtServer` for a mob attack leaves the stand whole: it is flung, not hurt.
        if w.get::<crate::stand::Stand>(c).is_none() {
            hurt_creature(w, c, e, r.damage);
        }
        push_creature(w, c, fling(from, creature_xz(w, c), r.fling));
    }
    w.resource_mut::<Events>().host.particles("poof", [at.x, at.y + 0.5, at.z], [0.5; 3], 0.0, 40);
}
