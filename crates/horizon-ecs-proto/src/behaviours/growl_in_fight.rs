//! `PiglinBruteAi.maybePlayActivitySound`, called by `PiglinBrute.customServerAiStep` on every server tick: a brute draws a float, and
//! below 0.0125 it plays `entity.piglin_brute.angry` at its place when its active activity is the fight (`playActivitySound`). The draw
//! happens every tick, whatever the activity. The float is `Rng::next_f64`, as the other chance rolls of the crate (one `next_u32`).
use super::*;
use crate::brain::is_active;
use crate::{cow::*, goals::*};

/// `ACTIVITY_SOUND_LIKELIHOOD_PER_TICK`.
pub const LIKELIHOOD: f32 = 0.0125;
/// The brute growls in its fight.
#[derive(Component, Clone, Copy, Debug)]
pub struct Growls;

pub static MODULE: Module = Module {
    name: "growl_in_fight",
    flags: 0,
    equip,
    fired: never,
    can_use: never,
    tick: Some(tick),
    requires: &[Cap::Body, Cap::Goals],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, _: Inst, _: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Growls);
}
/// A rule, not a goal: nothing starts it.
fn never(_: &mut World, _: Entity, _: Inst) -> bool {
    false
}
fn tick(w: &mut World) {
    let growls: Vec<Entity> = crate::queries::entities::<With<Growls>>(w);
    for e in growls {
        let Some(f) = w.get_mut::<Rng>(e).map(|mut r| r.next_f64()) else { continue };
        if f >= f64::from(LIKELIHOOD) || !is_active(w, e, "fight") {
            continue;
        }
        let at = w.get::<Pos>(e).unwrap().0;
        w.resource_mut::<Events>().host.sound("minecraft:entity.piglin_brute.angry", [at.x, at.y, at.z], 1.0, 1.0);
    }
}
