//! Aides partagées des comportements de bloc du villageois : position, bloc à portée tiré comme le `Mob`
//! (`pick_farmland`, `pick_bonemeal_crop` : réservoir sur le tirage du niveau, ordre x, y, z), marche, ciel
//! (`no_blocks_above`, `villager_brain.rs:2497-2529`).
use crate::blocks;
use crate::brain::*;
use bevy_ecs::prelude::*;
use crate::cow::Pos;
use crate::goals::with_rng;
use horizon_client_lib::mobs::MobWorld;
use horizon_core::geometry::Vec3;

/// A block test: the mob world and a block position.
pub type Pred = fn(&'static MobWorld<'static>, [i32; 3]) -> bool;

pub fn block_at(w: &World, e: Entity) -> Option<[i32; 3]> {
    let p = w.get::<Pos>(e)?.0;
    Some([p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32])
}

/// The squared distance from the centre of block `p` to `at`.
pub fn centre_dist_sq(p: [i32; 3], at: Vec3) -> f64 {
    Vec3::new(f64::from(p[0]) + 0.5, f64::from(p[1]) + 0.5, f64::from(p[2]) + 0.5).distance_squared(at)
}

/// One block of the 3x3x3 around the entity that passes `pred`, chosen uniformly by the entity's random.
pub fn pick_in_reach(w: &mut World, e: Entity, pred: Pred) -> Option<[i32; 3]> {
    let c = block_at(w, e)?;
    let mw = blocks::world_of(w);
    let cands: Vec<[i32; 3]> = (-1..=1)
        .flat_map(|x| (-1..=1).flat_map(move |y| (-1..=1).map(move |z| [c[0] + x, c[1] + y, c[2] + z])))
        .filter(|&q| pred(mw, q))
        .collect();
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

/// `no_blocks_above`: the block sees the sky and the motion-blocking height of its column is not above the entity.
pub fn no_blocks_above(mw: &MobWorld<'static>, p: [i32; 3], y: f64) -> bool {
    mw.can_see_sky(p[0], p[1], p[2]) && f64::from(mw.motion_blocking_height(p[0], p[2])) <= y
}
