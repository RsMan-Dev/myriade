//! Recherche de lit pour les bébés (`sense_bed`, `villager_brain.rs:690`) : lits dans 48 blocs, au plus 5 nouveaux essais
//! par passage, cache de 40 ticks par lit essayé. Le lit visé va dans `BED_CANDIDATE` (un `reach_block` le rejoint) ; il
//! n'entre dans `NEAREST_BED` qu'une fois atteint, comme le chemin `reached` du `Mob`.
use super::poi_claim::block_of;
use crate::brain::*;
use crate::cow::{now_of, GrowAt, Pos, Rng};
use crate::poi::PoiRegistry;
use horizon_world::poi::PoiType;

pub struct Params {
    pub nearest: &'static MemKey,
    pub candidate: &'static MemKey,
}

/// Lits essayés récemment : (position, jusqu'à quel tick). Seulement chez les bébés.
#[derive(Component, Clone, Default)]
#[component(storage = "SparseSet")]
pub struct BedCache(pub Vec<([i32; 3], i64)>);

fn is_home(k: PoiType) -> bool {
    k == PoiType::Home
}

fn scan(w: &mut World, e: Entity, c: &Ctx) {
    let p = c.get::<Params>();
    if w.get::<GrowAt>(e).is_none() {
        return;
    }
    let now = now_of(w, e) as i64;
    let last_update = now + w.get_mut::<Rng>(e).map_or(0, |mut r| i64::from(r.next_u32() % 20));
    let here = block_of(w, e);
    let homes: Vec<[i32; 3]> = w.resource::<PoiRegistry>().in_range(here, 48, false, is_home).into_iter().map(|h| h.0).collect();
    let mut cache = w.get::<BedCache>(e).cloned().unwrap_or_default();
    let mut tried = 0;
    let mut targets = Vec::new();
    for h in homes {
        if cache.0.iter().any(|(q, _)| *q == h) {
            continue;
        }
        tried += 1;
        if tried >= 5 {
            continue;
        }
        cache.0.push((h, last_update + 40));
        targets.push(h);
    }
    match targets.first() {
        Some(&t) => {
            set(w, e, p.candidate, MemValue::Block(t), None);
            if w.get::<Pos>(e).is_some_and(|q| {
                let (dx, dz) = (f64::from(t[0]) + 0.5 - q.0.x, f64::from(t[2]) + 0.5 - q.0.z);
                dx * dx + dz * dz <= 1.5 * 1.5
            }) {
                set(w, e, p.nearest, MemValue::Block(t), None);
            }
        }
        None if tried < 5 => cache.0.retain(|(_, until)| *until >= last_update),
        None => {}
    }
    w.entity_mut(e).insert(BedCache(cache.0));
}

pub static SENSOR: Sensor = Sensor { name: "nearest_bed", scan, ..Sensor::DEFAULT };
