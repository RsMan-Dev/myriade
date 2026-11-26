//! Prend un point d'intérêt libre (lit, poste de travail, point de rencontre) et le tient jusqu'à sa perte.
//! Réessai à attente croissante (`Retry`, comme `villager_brain.rs:1330`). La perte est événementielle : `lose_claims`
//! ne lit que les points sortis du registre (`removed()`) et n'atteint que les entités qui les tiennent (`ClaimIndex`).
use crate::blocks::world_of;
use crate::brain::*;
use bevy_ecs::prelude::*;
use crate::cow::{now_of, Pos, Rng};
use super::profession::Profession;
use super::trade_offers;
use crate::poi::PoiRegistry;
use crate::rules::poi_claim::retry_after;
use horizon_world::poi::PoiType;
use std::collections::HashMap;

/// Filtre en plus de `accept` : `FreeBed` saute les lits occupés (comme le Mob pour Home) ; `OwnJob` garde
/// les points du métier de l'entité s'il en a un (comme le Mob pour un villageois déjà employé).
#[derive(Clone, Copy, PartialEq)]
pub enum Filter {
    Any,
    FreeBed,
    OwnJob,
}

pub struct Params {
    pub memory: &'static MemKey,
    pub accept: fn(PoiType) -> bool,
    pub range: i32,
    pub filter: Filter,
    /// Appelé à la prise (ex. changement de métier) et à la perte (ex. remise à zéro du métier).
    pub on_acquired: fn(&mut World, Entity, [i32; 3]),
    pub on_lost: fn(&mut World, Entity),
}

/// Les prises d'un villageois : rare, sparse.
#[derive(Component, Clone)]
#[component(storage = "SparseSet", on_remove = release_claims)]
pub struct Claims(pub Vec<Claim>);

/// `Villager.die` / removal: the tickets still held go back to the registry (`releasePoi`), and the entity leaves the index.
fn release_claims(mut world: bevy_ecs::world::DeferredWorld, ctx: bevy_ecs::lifecycle::HookContext) {
    let Some(cs) = world.get::<Claims>(ctx.entity).cloned() else { return };
    for c in cs.0 {
        world.resource_mut::<PoiRegistry>().release(c.pos);
        if let Some(owners) = world.resource_mut::<ClaimIndex>().0.get_mut(&c.pos) {
            owners.retain(|&x| x != ctx.entity);
        }
    }
}

#[derive(Clone, Copy)]
pub struct Claim {
    pub pos: [i32; 3],
    pub memory: &'static MemKey,
    pub on_lost: fn(&mut World, Entity),
}

/// Le réessai d'une prise : `delay` croît de `rand(40) + 40`, plafonné à 400 ; oublié après 400 ticks sans essai.
#[derive(Clone, Copy, Default)]
pub struct Retry {
    pub previous: i64,
    pub next: i64,
    pub delay: i32,
}

/// Un réessai par mémoire visée (chaque `AcquirePoi` du `Mob` a le sien : le point de rencontre manquant ne retarde pas le lit).
#[derive(Component, Clone, Default)]
#[component(storage = "SparseSet")]
pub struct Retries(pub Vec<(&'static str, Retry)>);

fn retry_of(w: &World, e: Entity, key: &MemKey) -> Option<Retry> {
    w.get::<Retries>(e)?.0.iter().find(|(k, _)| *k == key.name).map(|x| x.1)
}

/// Index des prises par position : `lose_claims` ne visite que les entités concernées.
#[derive(Resource, Default)]
pub struct ClaimIndex(pub HashMap<[i32; 3], Vec<Entity>>);

/// Le bloc sous les pieds de `e`.
pub fn block_of(w: &World, e: Entity) -> [i32; 3] {
    w.get::<Pos>(e).map_or([0; 3], |p| [p.0.x.floor() as i32, p.0.y.floor() as i32, p.0.z.floor() as i32])
}

fn mark_retry(w: &mut World, e: Entity, key: &MemKey) {
    let now = now_of(w, e) as i64;
    let jitter = w.get_mut::<Rng>(e).map_or(0, |mut r| (r.next_u32() % 40) as i32);
    let mut r = retry_of(w, e, key).unwrap_or_default();
    (r.delay, r.next) = retry_after(now, r.previous, r.delay, jitter);
    r.previous = now;
    let mut all = w.get::<Retries>(e).cloned().unwrap_or_default();
    all.0.retain(|(k, _)| *k != key.name);
    all.0.push((key.name, r));
    w.entity_mut(e).insert(all);
}

/// Le premier point libre dans la portée, que `filter` laisse passer.
fn nearest(w: &World, e: Entity, p: &Params) -> Option<[i32; 3]> {
    if p.filter == Filter::OwnJob && !trade_offers::profession_known(w, e) {
        return None;
    }
    let found = w.resource::<PoiRegistry>().in_range(block_of(w, e), p.range, true, p.accept);
    let own = w.get::<Profession>(e).map(|j| j.0);
    found.into_iter().find_map(|(q, k)| {
        let ok = match p.filter {
            Filter::Any => true,
            Filter::FreeBed => world_of(w).prop(q[0], q[1], q[2], "occupied") != Some("true"),
            Filter::OwnJob => own.is_none_or(|j| j == k),
        };
        ok.then_some(q)
    })
}

fn can_start(w: &mut World, e: Entity, c: &Ctx) -> bool {
    let p = c.get::<Params>();
    if present(w, e, p.memory) || retry_of(w, e, p.memory).is_some_and(|r| (now_of(w, e) as i64) < r.next) {
        return false;
    }
    let free = nearest(w, e, p).is_some();
    if !free {
        mark_retry(w, e, p.memory);
    }
    free
}

fn start(w: &mut World, e: Entity, c: &Ctx) {
    let p = c.get::<Params>();
    let Some(poi) = nearest(w, e, p) else {
        return mark_retry(w, e, p.memory);
    };
    if !w.resource_mut::<PoiRegistry>().acquire(poi) {
        return mark_retry(w, e, p.memory);
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
