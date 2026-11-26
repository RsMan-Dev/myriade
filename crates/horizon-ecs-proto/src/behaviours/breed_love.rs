//! Amour et naissance (`MakeLove`, `villager_brain.rs:1636-1660`, `2923-2990`) : deux villageois qui peuvent se reproduire
//! restent ensemble ; l'enfant naît à `now + 275 + rand(50)`, les parents se reposent 6000 ticks, l'enfant grandit en 24000.
//! L'enfant reprend le lit le plus proche d'un parent (48 blocs, lit libre) ; des cœurs apparaissent (`events.host`).
use crate::behaviours::bed::{bed_taken, NEAREST_BED};
use crate::behaviours::food_stock;
use crate::brain::*;
use bevy_ecs::prelude::*;
use crate::cow::*;
use crate::goals::{self, Events};
use crate::behaviours::breed::Resting;
use crate::behaviours::Cap;
use crate::rules::breed_love::{birth_delay, too_far_to_mate};
use crate::targeting::Who;
use myriade_engine_lib::items::Items;

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

/// `MakeLove.start` : la naissance est planifiée (souvenir `BIRTH_DUE`, expire à l'échéance).
pub fn start(w: &mut World, e: Entity, partner: Entity) {
    let now = now_of(w, e);
    let delay = goals::with_rng(w, e, |r| birth_delay(r.next_int(50)));
    w.entity_mut(e).insert(Mating { partner, birth: now + delay });
    set(w, e, &BIRTH_DUE, MemValue::Unit, Some(delay));
}

/// `VillagerMakeLove.takeVacantBed` (`PoiManager.take(HOME, .., 48)`, jar) : le lit libre le plus proche du parent dans 48 blocs, que
/// ni un dormeur ni le lit retenu d'un autre villageois n'occupe (l'accessibilité par chemin n'est pas vérifiée).
fn child_bed(w: &mut World, parent: Entity, baby: Entity) -> Option<[i32; 3]> {
    let here = crate::behaviours::poi_claim::block_of(w, parent);
    let homes = w.resource::<crate::poi::PoiRegistry>().in_range(here, 48, true, |k| k == horizon_world::poi::PoiType::Home);
    let claimed: Vec<[i32; 3]> = w
        .query::<Entity>()
        .iter(w)
        .collect::<Vec<_>>()
        .into_iter()
        .filter(|e| *e != baby)
        .filter_map(|e| match get(w, e, &NEAREST_BED) {
            Some(MemValue::Block(b)) => Some(*b),
            _ => None,
        })
        .collect();
    homes.into_iter().map(|h| h.0).find(|b| !claimed.contains(b) && !bed_taken(w, baby, *b))
}
fn bed_for_child(w: &mut World, parent: Entity, baby: Entity) {
    if let Some(bed) = child_bed(w, parent, baby) {
        set(w, baby, &NEAREST_BED, MemValue::Block(bed), None);
    }
}

/// `Villager.getBreedOffspring`: one draw, below 0.5 the type of the biome (not read here: P has no biome facts, the child keeps the
/// type its spawn gave it), below 0.75 the first parent's type, else the partner's.
fn inherit_type(w: &mut World, baby: Entity, a: Entity, b: Entity) {
    let d = goals::with_rng(w, a, |r| r.next_f64());
    if d < 0.5 {
        return;
    }
    let from = if d < 0.75 { a } else { b };
    let Some(v) = w.get::<crate::behaviours::trade_offers::Merchant>(from).map(|m| m.variant) else { return };
    if let Some(mut m) = w.get_mut::<crate::behaviours::trade_offers::Merchant>(baby) {
        m.variant = v;
    }
    crate::bridge::view::mark(w, baby);
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
    // `VillagerMakeLove.breed`: the child is snapped to the first parent (the one whose brain ticks).
    let at = w.get::<Pos>(e).unwrap().0;
    let baby = spawn_like(w, e, at.x, at.z, seed);
    if let Some(mut p) = w.get_mut::<Pos>(baby) {
        p.0.y = at.y;
    }
    crate::species::goat::inherit_screaming(w, baby, e, m.partner);
    w.entity_mut(baby).insert(GrowAt(now + GROWN));
    crate::attr_mod::apply_load(w, baby);
    bed_for_child(w, e, baby);
    for p in [e, m.partner] {
        food_stock::digest(w, p);
        w.entity_mut(p).insert(Resting(now + REST));
    }
    inherit_type(w, baby, e, m.partner);
    // The child's brain equips its behaviours (`spawn_like` leaves that to the caller, as `breed::give_birth` does).
    crate::brain::start(w, baby);
    // Entity event 12 on the child (hearts).
    w.resource_mut::<Events>().host.entity_event(baby, 12);
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
    if too_far_to_mate(crate::goals::d2(a, b)) {
        return true;
    }
    lock_gaze(w, e, p);
    if now >= m.birth {
        // `takeVacantBed` found none: the birth carries no child (`VillagerBirth { child: None }`).
        if child_bed(w, e, e).is_none() {
            // `tryToGiveBirth`: entity event 13 (angry villager) for both, no child.
            let ev = &mut w.resource_mut::<Events>().host;
            ev.entity_event(p, 13);
            ev.entity_event(e, 13);
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
