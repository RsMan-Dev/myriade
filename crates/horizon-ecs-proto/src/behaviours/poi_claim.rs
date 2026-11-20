//! Prend un point d'intérêt libre (lit, poste de travail, point de rencontre) et le tient jusqu'à sa perte.
//! Réessai à attente croissante (`Retry`, comme `villager_brain.rs:1330`). La perte est événementielle : `lose_claims`
//! ne lit que les points sortis du registre (`removed()`) et n'atteint que les entités qui les tiennent (`ClaimIndex`).
use crate::brain::*;
use bevy_ecs::prelude::*;
use crate::cow::{now_of, Pos, Rng};
use crate::poi::PoiRegistry;
use horizon_world::poi::PoiType;
use std::collections::HashMap;

pub struct Params {
    pub memory: &'static MemKey,
    pub accept: fn(PoiType) -> bool,
    pub range: i32,
    /// Appelé à la prise (ex. changement de métier) et à la perte (ex. remise à zéro du métier).
    pub on_acquired: fn(&mut World, Entity, [i32; 3]),
    pub on_lost: fn(&mut World, Entity),
}

/// Les prises d'un villageois : rare, sparse.
#[derive(Component, Clone)]
#[component(storage = "SparseSet")]
pub struct Claims(pub Vec<Claim>);

#[derive(Clone, Copy)]
pub struct Claim {
    pub pos: [i32; 3],
    pub memory: &'static MemKey,
    pub on_lost: fn(&mut World, Entity),
}

/// Le réessai d'une prise : `delay` croît de `rand(40) + 40`, plafonné à 400 ; oublié après 400 ticks sans essai.
#[derive(Component, Clone, Copy, Default)]
#[component(storage = "SparseSet")]
pub struct Retry {
    pub previous: i64,
    pub next: i64,
    pub delay: i32,
}

/// Index des prises par position : `lose_claims` ne visite que les entités concernées.
#[derive(Resource, Default)]
pub struct ClaimIndex(pub HashMap<[i32; 3], Vec<Entity>>);

/// Le bloc sous les pieds de `e`.
pub fn block_of(w: &World, e: Entity) -> [i32; 3] {
    w.get::<Pos>(e).map_or([0; 3], |p| [p.0.x.floor() as i32, p.0.y.floor() as i32, p.0.z.floor() as i32])
}

fn mark_retry(w: &mut World, e: Entity) {
    let now = now_of(w, e) as i64;
    let jitter = w.get_mut::<Rng>(e).map_or(0, |mut r| (r.next_u32() % 40) as i32);
    let mut r = w.get::<Retry>(e).copied().unwrap_or_default();
    if now - r.previous >= 400 {
        r.delay = 0;
    }
    r.previous = now;
    r.delay = (r.delay + jitter + 40).min(400);
    r.next = now + i64::from(r.delay);
    w.entity_mut(e).insert(r);
}

fn can_start(w: &mut World, e: Entity, c: &Ctx) -> bool {
    let p = c.get::<Params>();
    if present(w, e, p.memory) || w.get::<Retry>(e).is_some_and(|r| (now_of(w, e) as i64) < r.next) {
        return false;
    }
    let free = !w.resource::<PoiRegistry>().in_range(block_of(w, e), p.range, true, p.accept).is_empty();
    if !free {
        mark_retry(w, e);
    }
    free
}

fn start(w: &mut World, e: Entity, c: &Ctx) {
    let p = c.get::<Params>();
    let here = block_of(w, e);
    let Some(&(poi, _)) = w.resource::<PoiRegistry>().in_range(here, p.range, true, p.accept).first() else {
        return mark_retry(w, e);
    };
    if !w.resource_mut::<PoiRegistry>().acquire(poi) {
        return mark_retry(w, e);
    }
    set(w, e, p.memory, MemValue::Block(poi), None);
    let claim = Claim { pos: poi, memory: p.memory, on_lost: p.on_lost };
    match w.get_mut::<Claims>(e) {
        Some(mut cs) => cs.0.push(claim),
        None => {
            w.entity_mut(e).insert(Claims(vec![claim]));
        }
    }
    w.resource_mut::<ClaimIndex>().0.entry(poi).or_default().push(e);
    (p.on_acquired)(w, e, poi);
}

pub static BEHAVIOUR: Behaviour = Behaviour { name: "poi:claim", one_shot: true, can_start, start, ..Behaviour::DEFAULT };

/// Système : ne tourne que sur les points sortis du registre ce tick. Pour chaque entité qui tenait un de ces points :
/// la mémoire s'efface, la prise sort, `on_lost` s'exécute. Aucune autre entité n'est touchée.
pub fn lose_claims(w: &mut World) {
    let lost = w.resource::<PoiRegistry>().removed().to_vec();
    for pos in lost {
        let Some(owners) = w.resource_mut::<ClaimIndex>().0.remove(&pos) else { continue };
        for e in owners {
            let Some(c) = w.get::<Claims>(e).and_then(|cs| cs.0.iter().find(|c| c.pos == pos).copied()) else { continue };
            if let Some(mut cs) = w.get_mut::<Claims>(e) {
                cs.0.retain(|x| x.pos != pos);
                if cs.0.is_empty() {
                    drop(cs);
                    w.entity_mut(e).remove::<Claims>();
                }
            }
            erase(w, e, c.memory);
            (c.on_lost)(w, e);
        }
    }
}
