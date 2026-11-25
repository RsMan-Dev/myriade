//! What an effect does when its bearer is removed by a death (`MobEffect.onMobRemoved(.., KILLED)`): Weaving leaves cobwebs about the body, Oozing slimes, Wind Charged a burst.
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

/// A body that died bearing Oozing.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Oozed;

/// A body that died bearing Wind Charged.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct WindBorne;

/// `OozingMobEffect.onMobRemoved`: 2 slimes of size 2, fewer when the slimes within 2 blocks of the body already fill `maxEntityCramming`
/// (`clamp(max - nearby, 0, 2)`; no limit when it is 0 or less). They come at the body's place, half a block up, with a random yaw.
pub fn oozing_on_killed(w: &mut World, e: Entity) {
    if w.get::<Oozed>(e).is_none() {
        return;
    }
    let (Some(p), Some(b)) = (w.get::<Pos>(e).map(|p| p.0), w.get::<crate::cow::Bounds>(e).map(|b| b.0)) else { return };
    let max = crate::cow::cramming_limit(w.resource::<crate::cow::Level>()).map_or(0, |m| m as i32);
    let mut count = 2;
    if max >= 1 {
        let zone = b.inflate(horizon_core::geometry::Vec3::new(2.0, 2.0, 2.0));
        let slime = crate::species::registry().id("minecraft:slime");
        let near = {
            let mut q = w.query::<(Entity, &crate::cow::SpeciesId, &crate::cow::Bounds)>();
            q.iter(w).filter(|(o, s, bb)| *o != e && Some(s.0 as usize) == slime && zone.intersects(bb.0)).count() as i32
        };
        count = (max - near.min(max)).clamp(0, 2);
    }
    let Some(sp) = w.resource::<crate::cow::SpeciesTable>().0.iter().find(|s| s.name == "minecraft:slime").copied() else { return };
    for _ in 0..count {
        let seed = crate::goals::with_rng(w, e, |r| r.next_u32()) as u64 | 1;
        let child = crate::size::spawn_sized(w, &sp, p.x, p.z, seed, 2);
        crate::cow::teleport(w, child, horizon_core::geometry::Vec3::new(p.x, p.y + 0.5, p.z));
        let yaw = crate::goals::with_rng(w, e, |r| r.next_f64()) as f32 * 360.0;
        if let Some(mut y) = w.get_mut::<crate::cow::Yaw>(child) {
            y.0 = yaw;
        }
        w.resource_mut::<crate::goals::Entered>().0.push(child);
    }
}

/// `WindChargedMobEffect.onMobRemoved`: a wind burst of radius `3 + 2 * nextFloat` at the middle of the body's height (no damage, no block).
pub fn wind_on_killed(w: &mut World, e: Entity) {
    if w.get::<WindBorne>(e).is_none() {
        return;
    }
    let (Some(p), Some(b)) = (w.get::<Pos>(e).map(|p| p.0), w.get::<crate::cow::Bounds>(e).map(|b| b.0)) else { return };
    let height = b.max_position().y - b.min_position().y;
    let radius = 3.0 + crate::goals::with_rng(w, e, |r| r.next_f64()) as f32 * 2.0;
    let at = horizon_core::geometry::Vec3::new(p.x, p.y + height / 2.0, p.z);
    crate::explosion::wind_burst(w, at, radius, Some(crate::targeting::Who::Entity(e)));
}
