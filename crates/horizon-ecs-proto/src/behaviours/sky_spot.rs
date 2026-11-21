//! Le villageois va sur une case qui voit le ciel pour fêter la victoire (`MoveToSkySeeingSpot`,
//! `villager_brain.rs:2497-2529`). Le Mob tire `tries` cases dans ±10, ±3, ±10 autour des pieds ; la première qui voit le ciel gagne.
use super::block_reach::{block_at, no_blocks_above, walk_to};
use crate::blocks;
use crate::brain::*;
use crate::goals::with_rng;
use bevy_ecs::prelude::*;

pub static BEHAVIOUR: Behaviour = Behaviour {
    name: "minecraft:villager_move_to_sky_spot",
    mems: &[&WALK_TARGET, &SPOT],
    one_shot: true,
    can_start,
    start,
    ..Behaviour::DEFAULT
};

pub struct Params {
    /// Number of random candidates drawn (10 for the villager).
    pub tries: usize,
    pub speed: f32,
}

fn spot(w: &mut World, e: Entity, tries: usize) -> Option<[i32; 3]> {
    let c = block_at(w, e)?;
    let mw = blocks::world_of(w);
    for _ in 0..tries {
        let mut q = c;
        for (k, d) in [10, 3, 10].into_iter().enumerate() {
            q[k] += with_rng(w, e, |r| r.next_int(2 * d + 1)) as i32 - d;
        }
        if no_blocks_above(mw, q, f64::from(c[1])) {
            return Some(q);
        }
    }
    None
}

/// The spot drawn by `can_start`, walked to by `start`: one draw per decision, as the Mob's `MoveToSkySeeingSpot`.
pub static SPOT: MemKey = MemKey::new("horizon:sky_spot", Shape::Block);

/// `MoveToSkySeeingSpot` refuses when a walk target is set or the villager already sees the sky; otherwise it runs.
fn can_start(w: &mut World, e: Entity, c: &Ctx) -> bool {
    if present(w, e, &WALK_TARGET) {
        return false;
    }
    let Some(here) = block_at(w, e) else { return false };
    if blocks::world_of(w).can_see_sky(here[0], here[1], here[2]) {
        return false;
    }
    if let Some(q) = spot(w, e, c.get::<Params>().tries) {
        set(w, e, &SPOT, MemValue::Block(q), None);
    }
    true
}

fn start(w: &mut World, e: Entity, c: &Ctx) {
    let p = c.get::<Params>();
    let Some(&MemValue::Block(q)) = get(w, e, &SPOT) else { return };
    erase(w, e, &SPOT);
    walk_to(w, e, q, p.speed, 0);
}
