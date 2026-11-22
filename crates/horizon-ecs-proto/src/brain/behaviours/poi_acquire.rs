//! `AcquirePoi` (`villager_brain.rs:1854`): the claim of `poi_claim` for a point of a filter, not made by a baby when `adult` (the `Mob`'s
//! `only_adult`), with the happy particles (entity event 14, `happy_particles`) when `event` and the claim is taken. The search, the
//! retry and the loss are `poi_claim`'s.
use crate::behaviours::poi_claim;
use crate::brain::*;
use crate::goals::Events;

pub struct Params {
    pub claim: &'static poi_claim::Params,
    pub adult: bool,
    pub event: bool,
    pub baby: fn(&World, Entity) -> bool,
}

/// `happy_particles` of the `Mob`: the clients play the happy villager particles around the entity.
pub fn happy_particles(w: &mut World, e: Entity) {
    w.resource_mut::<Events>().host.entity_event(e, 14);
}

fn inner(c: &Ctx, p: &'static Params) -> Ctx {
    Ctx { id: c.id, nth: c.nth, params: p.claim }
}

fn can_start(w: &mut World, e: Entity, c: &Ctx) -> bool {
    let p = c.get::<Params>();
    if p.adult && (p.baby)(w, e) {
        return false;
    }
    (poi_claim::BEHAVIOUR.can_start)(w, e, &inner(c, p))
}

fn start(w: &mut World, e: Entity, c: &Ctx) {
    let p = c.get::<Params>();
    (poi_claim::BEHAVIOUR.start)(w, e, &inner(c, p));
    if p.event && present(w, e, p.claim.memory) {
        happy_particles(w, e);
    }
}

pub static BEHAVIOUR: Behaviour = Behaviour { name: "minecraft:acquire_poi", one_shot: true, can_start, start, ..Behaviour::DEFAULT };
