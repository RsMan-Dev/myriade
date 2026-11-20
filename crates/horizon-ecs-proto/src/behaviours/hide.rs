//! Pendant un raid, le villageois va se cacher (`LocateHidingPlace` puis `SetHiddenState`, `villager_brain.rs:2530-2591`).
//! Les abris sont un paramètre (`homes`, interface supposée : `PoiType::Home` absent) ; l'alerte vient de `HEARD_BELL`
//! (`bell_heard.rs`) ou du statut de raid (`raid.rs`).
use super::block_reach::walk_to;
use super::bell_heard::HEARD_BELL;
use super::raid;
use crate::brain::*;
use bevy_ecs::prelude::*;
use crate::goals::with_rng;

pub static BEHAVIOUR: Behaviour = Behaviour {
    name: "minecraft:villager_hide",
    mems: &[&WALK_TARGET, &LOOK_TARGET],
    passive: true,
    can_start,
    start,
    stop,
    ..Behaviour::DEFAULT
};

pub struct Params {
    /// The homes within `radius` of the entity (`LocateHidingPlace`), nearest first.
    pub homes: fn(&World, Entity, i32) -> Vec<[i32; 3]>,
    pub radius: i32,
}

/// Hidden: the villager stays on its hiding place until the raid ends (`SetHiddenState`).
pub static HIDING: MemKey = MemKey::new("minecraft:hiding", Shape::Bool);

fn alarmed(w: &World, e: Entity) -> bool {
    raid::active(w, e) || present(w, e, &HEARD_BELL)
}

fn can_start(w: &mut World, e: Entity, c: &Ctx) -> bool {
    alarmed(w, e) && !present(w, e, &HIDING) && !(c.get::<Params>().homes)(w, e, c.get::<Params>().radius).is_empty()
}

fn start(w: &mut World, e: Entity, c: &Ctx) {
    let p = c.get::<Params>();
    let homes = (p.homes)(w, e, p.radius);
    if homes.is_empty() {
        return;
    }
    let i = with_rng(w, e, |r| r.next_int(homes.len() as i32)) as usize;
    walk_to(w, e, homes[i], 0.6, 1);
}

/// Arrived at the place: hidden until the alarm is over.
/// Hidden once the walk to the place has ended (the walk memory is gone); see `villager-divers.md` for the arrival test.
fn stop(w: &mut World, e: Entity, _: &Ctx) {
    erase(w, e, &WALK_TARGET);
    erase(w, e, &LOOK_TARGET);
    set(w, e, &HIDING, MemValue::Bool(true), None);
}
