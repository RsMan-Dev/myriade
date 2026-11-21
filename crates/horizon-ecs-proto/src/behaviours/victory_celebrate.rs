//! Victoire de raid : le villageois célèbre, parfois avec un feu d'artifice (`Celebrate`, `villager_brain.rs:2520` et
//! tick `3024`). Durée et cadence sont des paramètres (valeurs du `Mob` à relever aux lignes 2520 et 3024, voir
//! `villager-divers.md`). Les feux sont une ressource drainée par l'hôte (`villager_firework`, `creatures/mod.rs:3466`).
use bevy_ecs::prelude::Entity;
use bevy_ecs::prelude::World;
use super::raid;
use crate::brain::*;
use crate::cow::Pos;
use crate::goals::with_rng;
use crate::rules::victory_celebrate::{celebration_end, firework_at, is_over};
use bevy_ecs::prelude::{Component, Resource};

pub static BEHAVIOUR: Behaviour = Behaviour {
    name: "minecraft:villager_celebrate_victory",
    mems: &[],
    passive: true,
    can_start,
    start,
    tick,
    stop,
    ..Behaviour::DEFAULT
};

pub struct Params {
    pub duration: i64,
    /// One firework chance out of this many ticks.
    pub firework_every: i32,
}

#[derive(Component, Clone, Copy)]
pub struct Celebration {
    pub until: u64,
}

pub struct Firework {
    pub at: [f64; 3],
}

/// Fireworks to launch this tick, drained by the host.
#[derive(Resource, Default)]
pub struct Fireworks(pub Vec<Firework>);

fn can_start(w: &mut World, e: Entity, _: &Ctx) -> bool {
    raid::status(w, e) == raid::RAID_VICTORY && w.get::<Celebration>(e).is_none()
}

fn start(w: &mut World, e: Entity, c: &Ctx) {
    let now = crate::cow::now_of(w, e);
    let until = celebration_end(now, c.get::<Params>().duration);
    w.entity_mut(e).insert(Celebration { until });
}

fn tick(w: &mut World, e: Entity, c: &Ctx) -> bool {
    let Some(cel) = w.get::<Celebration>(e).copied() else { return false };
    let now = crate::cow::now_of(w, e);
    if is_over(now, cel.until) {
        return false;
    }
    let every = c.get::<Params>().firework_every;
    if with_rng(w, e, |r| r.next_int(every)) == 0 {
        if let Some(p) = w.get::<Pos>(e).map(|q| q.0) {
            let at = firework_at(p);
            if let Some(mut fw) = w.get_resource_mut::<Fireworks>() {
                fw.0.push(Firework { at });
            }
        }
    }
    true
}

fn stop(w: &mut World, e: Entity, _: &Ctx) {
    w.entity_mut(e).remove::<Celebration>();
}
