//! What an effect does when its bearer is removed by a death (`MobEffect.onMobRemoved(.., KILLED)`): Weaving leaves cobwebs about the body.
use crate::cow::{Pos, Rng};
use bevy_ecs::prelude::*;

/// A body that died bearing Weaving (its effects end with its death, the cobwebs come when it is removed).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Woven;

/// `WeavingMobEffect.spawnCobwebsRandomlyAround`: `2 + nextInt(2)` cobwebs, picked among 15 random blocks within one block of the bearer's
/// block, each a replaceable block over a sturdy top. Only for a mob when `mobGriefing` holds. A cobweb comes with level event 3018.
pub fn on_killed(w: &mut World, e: Entity) {
    if w.get::<Woven>(e).is_none() || !crate::level::can_grief(w, e) {
        return;
    }
    let (Some(p), Some(mut rng)) = (w.get::<Pos>(e).map(|p| p.0), w.get::<Rng>(e).copied()) else { return };
    let base = [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32];
    let wanted = 2 + rng.next_int(2) as usize;
    let mut chosen: Vec<[i32; 3]> = Vec::new();
    for _ in 0..15 {
        let at = [base[0] - 1 + rng.next_int(3), base[1] - 1 + rng.next_int(3), base[2] - 1 + rng.next_int(3)];
        if chosen.contains(&at) {
            continue;
        }
        let mw = crate::blocks::world_of(w);
        let here = mw.world.block(at[0], at[1], at[2]);
        let below = mw.world.block(at[0], at[1] - 1, at[2]);
        if (mw.facts.is_air(here) || mw.facts.get(here).replaceable) && mw.facts.get(below).sturdy_full & (1 << 1) != 0 {
            chosen.push(at);
            if chosen.len() >= wanted {
                break;
            }
        }
    }
    *w.get_mut::<Rng>(e).unwrap() = rng;
    for at in chosen {
        crate::blocks::set_state(w, at, "minecraft:cobweb", &[]);
        w.resource_mut::<crate::goals::Events>().host.level_event(3018, at, 0);
    }
}
