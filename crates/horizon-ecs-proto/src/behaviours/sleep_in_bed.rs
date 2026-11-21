//! Dort dans le lit `NEAREST_BED` : marche vers le lit, s'y couche, reste couché tant que l'activité de repos dure (le cadre
//! arrête le comportement quand son activité se termine, et `stop` le relève).
use super::bed::*;
use super::reach_block::block_centre;
use crate::brain::*;
use bevy_ecs::prelude::*;
use crate::cow::now_of;

pub struct Params {
    pub speed: f64,
}
impl Params {
    pub const DEFAULT: Self = Self { speed: 0.5 };
}

/// The villager's bed is its `home` (`SleepInBed` reads `MemoryModuleType.HOME`, jar); the nearest bed of a baby stands in for it
/// when the home is not held.
fn bed(w: &World, e: Entity) -> Option<[i32; 3]> {
    match get(w, e, &crate::species::villager::HOME).or_else(|| get(w, e, &NEAREST_BED))? {
        MemValue::Block(p) => Some(*p),
        _ => None,
    }
}
/// Frappé depuis moins de 40 ticks, la fenêtre de `hurt_by` (`hurt_by.rs`).
fn hurt_recently(w: &World, e: Entity) -> bool {
    w.get::<LastDamage>(e).is_some_and(|d| now_of(w, e).saturating_sub(d.at) <= 40)
}
fn can_start(w: &mut World, e: Entity, _: &Ctx) -> bool {
    bed(w, e).is_some() && w.get::<Sleeping>(e).is_none() && !hurt_recently(w, e)
}
fn start(w: &mut World, e: Entity, c: &Ctx) {
    if let Some(p) = bed(w, e) {
        walk_to(w, e, block_centre(p), c.get::<Params>().speed);
    }
}
fn tick(w: &mut World, e: Entity, c: &Ctx) -> bool {
    // `SleepInBed.canStillUse`: the REST activity must be the active one (the brain stops nothing at an activity change).
    if !crate::brain::is_active(w, e, "rest") {
        return false;
    }
    if w.get::<Sleeping>(e).is_some() {
        // Frappé au lit : le villageois se réveille (`stop` retire `Sleeping`).
        return !hurt_recently(w, e);
    }
    let Some(p) = bed(w, e) else { return false };
    // `SleepInBed.checkExtraStartConditions`: the bed's centre within 2 blocks; short of it, the walk goes on (or starts again).
    let close = w.get::<crate::cow::Pos>(e).is_some_and(|at| {
        let c = [f64::from(p[0]) + 0.5, f64::from(p[1]) + 0.5, f64::from(p[2]) + 0.5];
        (at.0.x - c[0]).powi(2) + (at.0.y - c[1]).powi(2) + (at.0.z - c[2]).powi(2) < 4.0
    });
    if !close {
        if nav_done(w, e) {
            walk_to(w, e, block_centre(p), c.get::<Params>().speed);
        }
        return true;
    }
    if bed_taken(w, e, p) {
        return false;
    }
    lie_down(w, e, p);
    true
}
fn stop(w: &mut World, e: Entity, _: &Ctx) {
    w.entity_mut(e).remove::<Sleeping>();
}

pub static BEHAVIOUR: Behaviour = Behaviour { name: "sleep_in_bed", mems: &[&NEAREST_BED, &crate::species::villager::HOME], can_start, start, tick, stop, ..Behaviour::DEFAULT };
