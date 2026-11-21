//! `ValidatePoi` (`villager_brain.rs:1822`, `validate_poi`): within 16 blocks of the villager, the point a memory holds is dropped when it
//! is gone or no longer accepted, or (`bed`) when it is an occupied bed. `held`: the point must be the job of the villager's profession
//! (`PoiFilter::Held`). Farther away nothing happens. `forget` is the one way a memory of a claimed point is dropped.
use crate::behaviours::bed::Sleeping;
use crate::behaviours::poi_claim::{ClaimIndex, Claims};
use crate::behaviours::profession::Profession;
use crate::blocks::world_of;
use crate::brain::*;
use crate::cow::Pos;
use crate::poi::PoiRegistry;
use bevy_ecs::prelude::*;
use crate::rules::poi_validate as rules;
use horizon_world::poi::PoiType;

pub struct Params {
    pub memory: &'static MemKey,
    pub accept: fn(PoiType) -> bool,
    pub held: bool,
    pub bed: bool,
}

fn near(w: &World, e: Entity, p: [i32; 3]) -> bool {
    w.get::<Pos>(e).is_some_and(|at| rules::near(p, at.0))
}

fn stale(w: &World, e: Entity, c: &Params, p: [i32; 3]) -> bool {
    let Some(kind) = w.resource::<PoiRegistry>().get(p).map(|poi| poi.kind) else { return true };
    let other = !(c.accept)(kind) || (c.held && w.get::<Profession>(e).map(|pr| pr.0) != Some(kind));
    let taken = c.bed && world_of(w).prop(p[0], p[1], p[2], "occupied") == Some("true") && w.get::<Sleeping>(e).is_none();
    other || taken
}

/// Drops the memory of a point and the claim that holds it (`Claims`, `ClaimIndex`), and releases the point's ticket in the registry.
pub fn forget(w: &mut World, e: Entity, memory: &'static MemKey) {
    let p = match get(w, e, memory) {
        Some(MemValue::Block(p)) => *p,
        _ => return,
    };
    erase(w, e, memory);
    let mut held = false;
    if let Some(mut cs) = w.get_mut::<Claims>(e) {
        let before = cs.0.len();
        cs.0.retain(|c| !(c.pos == p && std::ptr::eq(c.memory, memory)));
        held = cs.0.len() != before;
    }
    if held {
        if w.get::<Claims>(e).is_some_and(|cs| cs.0.is_empty()) {
            w.entity_mut(e).remove::<Claims>();
        }
        let index = &mut w.resource_mut::<ClaimIndex>().0;
        if let Some(owners) = index.get_mut(&p) {
            owners.retain(|&x| x != e);
            if owners.is_empty() {
                index.remove(&p);
            }
        }
        w.resource_mut::<PoiRegistry>().release(p);
    }
}

fn can_start(w: &mut World, e: Entity, c: &Ctx) -> bool {
    let p = c.get::<Params>();
    match get(w, e, p.memory) {
        Some(MemValue::Block(q)) => near(w, e, *q) && stale(w, e, p, *q),
        _ => false,
    }
}

fn start(w: &mut World, e: Entity, c: &Ctx) {
    forget(w, e, c.get::<Params>().memory);
}

pub static BEHAVIOUR: Behaviour = Behaviour { name: "minecraft:validate_poi", one_shot: true, can_start, start, ..Behaviour::DEFAULT };
