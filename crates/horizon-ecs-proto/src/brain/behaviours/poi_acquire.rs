//! `AcquirePoi` (`villager_brain.rs:1854`): the claim of `poi_claim` for a point of a filter, not made by a baby when `adult` (the `Mob`'s
//! `only_adult`), with the happy particles (entity event 14, `happy_particles`) when `event` and the claim is taken. The search, the
//! retry and the loss are `poi_claim`'s.
use crate::behaviours::poi_claim;
use crate::brain::*;
use crate::goals::{self, Events};

/// `AcquirePoi`'s `nextScheduledStart`, one per entry of the plan (its `Ctx::id`): the tick of the next search; none before the first one.
#[derive(Component, Clone, Debug, Default)]
#[component(storage = "SparseSet")]
pub struct Scan(pub Vec<(u16, u64)>);

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
    // The schedule is tested before the search: a search runs only when due, then the next one is set 20 to 39 ticks on.
    due(w, e, c.id) && (poi_claim::BEHAVIOUR.can_start)(w, e, &inner(c, p))
}

/// `AcquirePoi.create`'s gate: the first call only draws `nextInt(20)`; later, a call is due at the scheduled tick.
fn due(w: &mut World, e: Entity, id: u16) -> bool {
    let now = now_of(w, e);
    let next = w.get::<Scan>(e).and_then(|s| s.0.iter().find(|x| x.0 == id).map(|x| x.1));
    let (at, due) = match next {
        None => (now + goals::with_rng(w, e, |r| r.next_int(20)) as u64, false),
        Some(n) if now < n => return false,
        Some(_) => (now + 20 + goals::with_rng(w, e, |r| r.next_int(20)) as u64, true),
    };
    let mut scan = w.get::<Scan>(e).cloned().unwrap_or(Scan(Vec::new()));
    scan.0.retain(|x| x.0 != id);
    scan.0.push((id, at));
    w.entity_mut(e).insert(scan);
    due
}

fn start(w: &mut World, e: Entity, c: &Ctx) {
    let p = c.get::<Params>();
    (poi_claim::BEHAVIOUR.start)(w, e, &inner(c, p));
    if p.event && present(w, e, p.claim.memory) {
        happy_particles(w, e);
    }
}

pub static BEHAVIOUR: Behaviour = Behaviour { name: "minecraft:acquire_poi", one_shot: true, can_start, start, ..Behaviour::DEFAULT };
