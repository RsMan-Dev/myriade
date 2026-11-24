//! `ServerPlayer.tellNeutralMobsThatIDied` and `NeutralMob.playerDied`: the mobs whose box meets the dead player's block inflated by
//! 32 across and 10 up and down (`AABB(blockPosition).inflate(32, 10, 32)`) and that are neutral (here: angry, `neutral_anger`) stop
//! being angry at that player (`stopBeingAngry`). The gamerule `forgive_dead_players` is the host's check. Written at the death; nothing polls.
use crate::behaviours::neutral_anger::{anger_at, stop_being_angry, Anger};
use crate::cow::{Body, Pos};
use crate::targeting::Who;
use bevy_ecs::prelude::*;

/// Forgives `player` at the entity `e`: true when its anger was for that player and has ended.
pub fn forgive(w: &mut World, e: Entity, player: u32) -> bool {
    if anger_at(w, e) != Some(Who::Player(player)) {
        return false;
    }
    stop_being_angry(w, e);
    true
}

/// Forgives `player` at every neutral mob in reach of the block `block` (the dead player's feet, floored).
pub fn forgive_in_reach(w: &mut World, block: [i32; 3], player: u32) {
    let angry: Vec<Entity> = crate::queries::entities::<With<Anger>>(w);
    for e in angry {
        if in_reach(w, e, block) {
            forgive(w, e, player);
        }
    }
}

/// Whether the box of `e` (width and height from its `Body`, strict overlaps as `AABB.intersects`) meets the block's box inflated.
fn in_reach(w: &World, e: Entity, block: [i32; 3]) -> bool {
    let (Some(p), Some(b)) = (w.get::<Pos>(e).map(|p| p.0), w.get::<Body>(e).copied()) else { return false };
    let half = f64::from(b.width) / 2.0;
    let (bx, by, bz) = (f64::from(block[0]), f64::from(block[1]), f64::from(block[2]));
    let (min, max) = ([bx - 32.0, by - 10.0, bz - 32.0], [bx + 1.0 + 32.0, by + 1.0 + 10.0, bz + 1.0 + 32.0]);
    let (lo, hi) = ([p.x - half, p.y, p.z - half], [p.x + half, p.y + f64::from(b.height), p.z + half]);
    (0..3).all(|i| lo[i] < max[i] && hi[i] > min[i])
}
