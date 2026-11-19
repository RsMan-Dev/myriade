//! Le fermier récolte une culture mûre à portée (3x3x3, tirée comme le `Mob`) et la replante (`HarvestFarmland`,
//! `villager_brain.rs:1685-1712`, tick `2998-3010`). `ripe` et `replant` viennent de la configuration (interfaces
//! supposées, voir `villager-divers.md`).
use super::block_reach::{block_at, look_block, pick_in_reach, walk_to, Pred};
use crate::behaviours::Cap;
use crate::blocks;
use crate::brain::*;
use horizon_client_lib::mobs::MobWorld;
use horizon_world::registry::BlockStateId;

pub static BEHAVIOUR: Behaviour = Behaviour {
    name: "minecraft:villager_harvest_crop",
    mems: &[&WALK_TARGET, &LOOK_TARGET],
    requires: &[Cap::Body, Cap::Locomotion],
    can_start,
    start,
    tick,
    stop,
    ..Behaviour::DEFAULT
};

pub struct Params {
    pub ripe: Pred,
    pub replant: fn(&'static MobWorld<'static>, [i32; 3]) -> Option<BlockStateId>,
}

fn can_start(w: &mut World, e: Entity, c: &Ctx) -> bool {
    let ripe = c.get::<Params>().ripe;
    pick_in_reach(w, e, ripe).is_some()
}

fn start(w: &mut World, e: Entity, c: &Ctx) {
    let ripe = c.get::<Params>().ripe;
    let Some(q) = pick_in_reach(w, e, ripe) else { return };
    walk_to(w, e, q, 0.5, 1);
    look_block(w, e, q);
}

/// Within reach (one block on each axis) and still mature: replant it and stop; otherwise keep walking.
fn tick(w: &mut World, e: Entity, c: &Ctx) -> bool {
    let p = c.get::<Params>();
    let Some(WalkTarget { target: Tracker::Block(q), .. }) = walk_target(w, e) else { return false };
    let mw = blocks::world_of(w);
    if !(p.ripe)(mw, q) {
        return false;
    }
    let Some(here) = block_at(w, e) else { return false };
    if (0..3).all(|i| (here[i] - q[i]).abs() <= 1) {
        if let Some(id) = (p.replant)(mw, q) {
            blocks::set_state_id(w, q, id);
        }
        return false;
    }
    true
}

fn stop(w: &mut World, e: Entity, _: &Ctx) {
    erase(w, e, &WALK_TARGET);
    erase(w, e, &LOOK_TARGET);
}
