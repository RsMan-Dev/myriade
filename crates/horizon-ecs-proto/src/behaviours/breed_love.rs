//! Amour et naissance (`MakeLove`, `villager_brain.rs:1636-1660`, `2923-2990`) : deux villageois qui peuvent se reproduire
//! restent ensemble ; l'enfant naît à `now + 275 + rand(50)`, les parents se reposent 6000 ticks, l'enfant grandit en 24000.
//! L'enfant reprend le lit le plus proche d'un parent (48 blocs, lit libre) ; des cœurs apparaissent (`events.host`).
use crate::behaviours::bed::{bed_taken, NEAREST_BED};
use crate::behaviours::{food_inventory, food_stock};
use crate::brain::*;
use bevy_ecs::prelude::*;
use crate::cow::*;
use crate::goals::{self, Events};
use crate::behaviours::breed::Resting;
use horizon_client_lib::items::Items;

/// Le couple : partenaire et temps de naissance.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Mating {
    pub partner: Entity,
    pub birth: u64,
}
/// Présent jusqu'à la naissance : il réveille le cerveau à l'échéance.
pub static BIRTH_DUE: MemKey = MemKey::new("horizon:birth_due", Shape::Unit);

pub const REST: u64 = 6000;
pub const GROWN: u64 = 24000;
/// `villager.findNearestBed` puis rayon de recherche du lit de l'enfant (`nearest_poi`, 48 blocs).
const BED_RANGE: f64 = 48.0;

/// `MakeLove.start` : la naissance est planifiée (souvenir `BIRTH_DUE`, expire à l'échéance).
pub fn start(w: &mut World, e: Entity, partner: Entity) {
    let now = now_of(w, e);
    let delay = goals::with_rng(w, e, |r| 275 + r.next_int(50) as u64);
    w.entity_mut(e).insert(Mating { partner, birth: now + delay });
    set(w, e, &BIRTH_DUE, MemValue::Unit, Some(delay));
}

/// Le lit de l'enfant : le lit le plus proche du parent, à 48 blocs au plus (XZ), s'il n'est pas occupé.
fn bed_for_child(w: &mut World, parent: Entity, baby: Entity) {
    let bed = match get(w, parent, &NEAREST_BED) {
        Some(MemValue::Block(p)) => *p,
        _ => return,
    };
    let at = w.get::<Pos>(parent).unwrap().0;
    let (dx, dz) = (f64::from(bed[0]) + 0.5 - at.x, f64::from(bed[2]) + 0.5 - at.z);
    if dx * dx + dz * dz > BED_RANGE * BED_RANGE || bed_taken(w, baby, bed) {
        return;
    }
    set(w, baby, &NEAREST_BED, MemValue::Block(bed), None);
}

/// `villagers.villager_birth` : l'enfant apparaît près du partenaire, grandit dans 24000 ticks, les parents se reposent.
/// Rend l'enfant si la naissance est due et que les deux peuvent se reproduire (sinon `None`, et l'union prend fin).
pub fn birth(w: &mut World, e: Entity, items: &Items) -> Option<Entity> {
    let now = now_of(w, e);
    let m = *w.get::<Mating>(e)?;
    if now < m.birth {
        return None;
    }
    w.entity_mut(e).remove::<Mating>();
    if !(food_stock::can_breed(w, e, items) && food_stock::can_breed(w, m.partner, items)) {
        return None;
    }
    let seed = goals::with_rng(w, e, |r| r.next_u32() as u64);
    let at = w.get::<Pos>(m.partner).unwrap().0;
    let baby = spawn_like(w, e, at.x, at.z, seed);
    crate::species::goat::inherit_screaming(w, baby, e, m.partner);
    w.entity_mut(baby).insert(GrowAt(now + GROWN));
    bed_for_child(w, e, baby);
    for p in [e, m.partner] {
        food_stock::digest(w, p);
        w.entity_mut(p).insert(Resting(now + REST));
    }
    let heart = [at.x, at.y + 1.0, at.z];
    w.resource_mut::<Events>().host.particles("minecraft:heart", heart, [0.5, 0.5, 0.5], 0.02, 7);
    Some(baby)
}

fn can_start(w: &mut World, e: Entity, _: &Ctx) -> bool {
    w.get::<Mating>(e).is_some_and(|m| now_of(w, e) >= m.birth)
}
fn start_birth(w: &mut World, e: Entity, _: &Ctx) {
    birth(w, e, food_stock::items(w));
}

pub static BIRTH: Behaviour = Behaviour {
    name: "breed:birth",
    mems: &[&BIRTH_DUE],
    one_shot: true,
    can_start,
    start: start_birth,
    ..Behaviour::DEFAULT
};
