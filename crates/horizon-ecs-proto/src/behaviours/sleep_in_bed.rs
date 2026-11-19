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

fn bed(w: &World, e: Entity) -> Option<[i32; 3]> {
    match get(w, e, &NEAREST_BED)? {
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
fn tick(w: &mut World, e: Entity, _: &Ctx) -> bool {
    if w.get::<Sleeping>(e).is_some() {
        // Frappé au lit : le villageois se réveille (`stop` retire `Sleeping`).
        return !hurt_recently(w, e);
    }
    let Some(p) = bed(w, e) else { return false };
    if !nav_done(w, e) {
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

pub static BEHAVIOUR: Behaviour = Behaviour { name: "sleep_in_bed", mems: &[&NEAREST_BED], can_start, start, tick, stop, ..Behaviour::DEFAULT };
