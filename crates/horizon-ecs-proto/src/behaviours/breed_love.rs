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
use crate::behaviours::Cap;
use crate::targeting::Who;
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
fn child_bed(w: &mut World, parent: Entity, baby: Entity) -> Option<[i32; 3]> {
    let Some(MemValue::Block(bed)) = get(w, parent, &NEAREST_BED).cloned() else { return None };
    let at = w.get::<Pos>(parent)?.0;
    let (dx, dz) = (f64::from(bed[0]) + 0.5 - at.x, f64::from(bed[2]) + 0.5 - at.z);
    (dx * dx + dz * dz <= BED_RANGE * BED_RANGE && !bed_taken(w, baby, bed)).then_some(bed)
}
fn bed_for_child(w: &mut World, parent: Entity, baby: Entity) {
    if let Some(bed) = child_bed(w, parent, baby) {
        set(w, baby, &NEAREST_BED, MemValue::Block(bed), None);
    }
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
    // With a `breed_target` the birth is `MAKE_LOVE`'s, at less than 5 blocks; this one is the forced union's.
    !present(w, e, &BREED_TARGET) && w.get::<Mating>(e).is_some_and(|m| now_of(w, e) >= m.birth)
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

fn partner_of(w: &World, e: Entity) -> Option<Entity> {
    match get(w, e, &BREED_TARGET) {
        Some(MemValue::Who(Who::Entity(p))) => Some(*p),
        _ => None,
    }
}
fn lock_gaze(w: &mut World, a: Entity, b: Entity) {
    for (x, y) in [(a, b), (b, a)] {
        let t = Tracker::Entity(Who::Entity(y), true);
        set(w, x, &LOOK_TARGET, MemValue::Look(t), None);
        set(w, x, &WALK_TARGET, MemValue::Walk(WalkTarget { target: t, speed: 0.5, close: 2 }), None);
    }
}
/// `VillagerMakeLove` (`villager_brain.rs:1636`, ordered gate of the idle activity on `breed_target`, 350 ticks): the birth is planned
/// at the start; each tick it stops when the date is passed or the two can no longer breed, else both look at each other and walk
/// together, and at less than 5 blocks (squared) from the date on the baby is born.
fn can_make(w: &mut World, e: Entity, _: &Ctx) -> bool {
    partner_of(w, e).is_some()
}
fn make_start(w: &mut World, e: Entity, _: &Ctx) {
    if let Some(p) = partner_of(w, e) {
        start(w, e, p);
    }
}
fn make_tick(w: &mut World, e: Entity, _: &Ctx) -> bool {
    let (Some(p), Some(m)) = (partner_of(w, e), w.get::<Mating>(e).copied()) else { return false };
    let now = now_of(w, e);
    if now > m.birth || !(crate::brain::behaviours::work_breed::ready(w, e) && crate::brain::behaviours::work_breed::ready(w, p)) {
        return false;
    }
    let (Some(a), Some(b)) = (w.get::<Pos>(e).map(|q| q.0), w.get::<Pos>(p).map(|q| q.0)) else { return false };
    if crate::goals::d2(a, b) > 5.0 {
        return true;
    }
    lock_gaze(w, e, p);
    if now >= m.birth {
        // `takeVacantBed` found none: the birth carries no child (`VillagerBirth { child: None }`).
        if child_bed(w, e, e).is_none() {
            return false;
        }
        let items = food_stock::items(w);
        birth(w, e, items);
        return false;
    }
    if goals::with_rng(w, e, |r| r.next_int(35)) == 0 {
        let ev = &mut w.resource_mut::<Events>().host;
        for at in [a, b] {
            ev.particles("minecraft:heart", [at.x, at.y + 1.0, at.z], [0.5, 0.5, 0.5], 0.02, 7);
        }
    }
    true
}
fn make_stop(w: &mut World, e: Entity, _: &Ctx) {
    w.entity_mut(e).remove::<Mating>();
    erase(w, e, &BREED_TARGET);
    erase(w, e, &WALK_TARGET);
    erase(w, e, &LOOK_TARGET);
}
pub static MAKE_LOVE: Behaviour = Behaviour {
    name: "minecraft:villager_make_love",
    mems: &[&BREED_TARGET, &WALK_TARGET, &LOOK_TARGET, &BIRTH_DUE],
    requires: &[Cap::Brain, Cap::Locomotion],
    can_start: can_make,
    start: make_start,
    tick: make_tick,
    stop: make_stop,
    ..Behaviour::DEFAULT
};
