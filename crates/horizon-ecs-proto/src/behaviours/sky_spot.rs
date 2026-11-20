//! Le villageois va sur une case qui voit le ciel pour fêter la victoire (`MoveToSkySeeingSpot`,
//! `villager_brain.rs:2497-2529`). Le prédicat est `block_reach::no_blocks_above` ; la hauteur de référence est celle
//! de la case visée (approximation, voir `villager-divers.md`).
use super::block_reach::{no_blocks_above, pick_in_reach, walk_to};
use crate::brain::*;
use bevy_ecs::prelude::*;
use horizon_client_lib::mobs::MobWorld;

pub static BEHAVIOUR: Behaviour = Behaviour {
    name: "minecraft:villager_move_to_sky_spot",
    mems: &[&WALK_TARGET],
    one_shot: true,
    can_start,
    start,
    ..Behaviour::DEFAULT
};

fn sees_sky(mw: &'static MobWorld<'static>, p: [i32; 3]) -> bool {
    no_blocks_above(mw, p, f64::from(p[1]))
}

fn can_start(w: &mut World, e: Entity, _: &Ctx) -> bool {
    !present(w, e, &WALK_TARGET) && pick_in_reach(w, e, sees_sky).is_some()
}

fn start(w: &mut World, e: Entity, _: &Ctx) {
    let Some(q) = pick_in_reach(w, e, sees_sky) else { return };
    walk_to(w, e, q, 0.6, 0);
}
