//! `InsideBrownianWalk` (`villager_brain.rs:2278`): a shuffled 3x3x3 search for a solid block under a roof.
use crate::behaviours::block_reach::block_at;
use crate::blocks;
use crate::brain::*;
use crate::goals::with_rng;
use crate::rules::village_brownian_walk::neighbours;

pub struct Params {
    pub speed: f32,
}

pub static BEHAVIOUR: Behaviour = Behaviour { name: "minecraft:inside_brownian_walk", mems: &[&WALK_TARGET], one_shot: true, can_start, ..Behaviour::DEFAULT };

fn can_start(w: &mut World, e: Entity, c: &Ctx) -> bool {
    if present(w, e, &WALK_TARGET) {
        return false;
    }
    let Some(here) = block_at(w, e) else { return false };
    let mw = blocks::world_of(w);
    if mw.can_see_sky(here[0], here[1], here[2]) {
        return false;
    }
    let mut poses = neighbours(here);
    for i in (1..poses.len()).rev() {
        let j = with_rng(w, e, |r| r.next_int(i as i32 + 1)) as usize;
        poses.swap(i, j);
    }
    if let Some(p) = poses.into_iter().find(|p| !mw.can_see_sky(p[0], p[1], p[2]) && mw.solid(p[0], p[1], p[2])) {
        set(w, e, &WALK_TARGET, MemValue::Walk(WalkTarget { target: Tracker::Block(p), speed: c.get::<Params>().speed, close: 0 }), None);
    }
    true
}
