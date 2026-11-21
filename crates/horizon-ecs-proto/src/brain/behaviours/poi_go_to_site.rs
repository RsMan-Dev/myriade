//! `GoToPotentialJobSite` (`villager_brain.rs:4058` in the core package, `:2746` tick, `:3138` stop): while a potential job site is
//! remembered, the villager walks to it and looks at it (speed 0.5, close 1) for the species' duration. Its end, or the loss of the
//! site, clears the memory and its claim (`forget`).
use crate::behaviours::block_reach::{look_block, walk_to};
use crate::brain::behaviours::poi_validate::forget;
use crate::brain::*;
use bevy_ecs::prelude::*;

pub struct Params {
    pub potential: &'static MemKey,
    pub speed: f32,
}

fn site(w: &World, e: Entity, k: &MemKey) -> Option<[i32; 3]> {
    match get(w, e, k) {
        Some(MemValue::Block(p)) => Some(*p),
        _ => None,
    }
}

fn aim(w: &mut World, e: Entity, c: &Params) {
    if let Some(p) = site(w, e, c.potential) {
        walk_to(w, e, p, c.speed, 1);
        look_block(w, e, p);
    }
}

fn can_start(w: &mut World, e: Entity, c: &Ctx) -> bool {
    present(w, e, c.get::<Params>().potential)
}

fn start(w: &mut World, e: Entity, c: &Ctx) {
    aim(w, e, c.get::<Params>());
}

fn tick(w: &mut World, e: Entity, c: &Ctx) -> bool {
    let p = c.get::<Params>();
    aim(w, e, p);
    present(w, e, p.potential)
}

fn stop(w: &mut World, e: Entity, c: &Ctx) {
    forget(w, e, c.get::<Params>().potential);
}

pub static BEHAVIOUR: Behaviour = Behaviour { name: "minecraft:go_to_potential_job_site", can_start, start, tick, stop, ..Behaviour::DEFAULT };
