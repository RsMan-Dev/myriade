//! Aides partagées des comportements de bloc du villageois : position, bloc à portée tiré comme le `Mob`
//! (`pick_farmland`, `pick_bonemeal_crop` : réservoir sur le tirage du niveau, ordre x, y, z), marche, ciel
//! (`no_blocks_above`, `villager_brain.rs:2497-2529`).
use crate::blocks;
use crate::brain::*;
use bevy_ecs::prelude::*;
use crate::cow::Pos;
use crate::goals::with_rng;
use myriade_engine_lib::mobs::MobWorld;
use horizon_core::geometry::Vec3;
use crate::rules::block_reach::{block_of, centre, neighbourhood};
pub use crate::rules::block_reach::no_blocks_above;

/// A block test: the mob world and a block position.
pub type Pred = fn(&'static MobWorld<'static>, [i32; 3]) -> bool;

pub fn block_at(w: &World, e: Entity) -> Option<[i32; 3]> {
    let p = w.get::<Pos>(e)?.0;
    Some(block_of(p.x, p.y, p.z))
}

/// The squared distance from the centre of block `p` to `at`.
pub fn centre_dist_sq(p: [i32; 3], at: Vec3) -> f64 {
    centre(p).distance_squared(at)
}

/// One block of the 3x3x3 around the entity that passes `pred`, chosen uniformly by the entity's random.
pub fn pick_in_reach(w: &mut World, e: Entity, pred: Pred) -> Option<[i32; 3]> {
    let c = block_at(w, e)?;
    let mw = blocks::world_of(w);
    let cands: Vec<[i32; 3]> = neighbourhood(c).into_iter().filter(|&q| pred(mw, q)).collect();
    if cands.is_empty() {
        return None;
    }
    let i = with_rng(w, e, |r| r.next_int(cands.len() as i32)) as usize;
    Some(cands[i])
}

/// Walks to block `p` (`WalkTarget.block`).
pub fn walk_to(w: &mut World, e: Entity, p: [i32; 3], speed: f32, close: i32) {
    set(w, e, &WALK_TARGET, MemValue::Walk(WalkTarget { target: Tracker::Block(p), speed, close }), None);
}

/// Looks at block `p` (`look_target`).
pub fn look_block(w: &mut World, e: Entity, p: [i32; 3]) {
    set(w, e, &LOOK_TARGET, MemValue::Look(Tracker::Block(p)), None);
}
