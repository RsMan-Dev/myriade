//! The waits of a hook in the water as deadlines on the wheel: the lure's wait and the fish's approach are vanilla's counters
//! (`timeUntilLured`, `timeUntilHooked`) read as dates in the hook's own time, not decremented per tick. The lure's splashes are
//! the next success of the vanilla chance per tick (a geometric draw, planned up to the segment where the chance holds). A change
//! of the fish's speed (rain, sky) moves the dates by what is left. The approach carries `Approach`: its hook is visited each
//! tick for the trail, the lure's hook is not visited between its dates.
//! Ecart (invisible, sans oscillation serveur) : le flotteur en attente est posé à son repos (`rest_height`, vitesse nulle) à la pose de `Wait` et n'est plus visité par tick ; il n'y revient pas sur événement (bloc changé, touche, retrait) : le repos n'est pas repris.
use crate::cow::*;
use horizon_core::geometry::Vec3;
use crate::fishing_hook::{bite_due, lure_splash, Hook};
use crate::goals::Player;
use crate::items::item_id;
use crate::rules::fishing_hook::lure_tease;
use crate::rules::hook_wait::{geometric_after, hooked_after, ticks_at};
use bevy_ecs::prelude::*;
use std::collections::HashMap;

const NEVER: u64 = u64::MAX;

/// The fish's counter of a hook in the water, `left` at tick `stamp` of the hook's own time, moving `speed` a tick: the lure's
/// (`approach` false) or the approach's. `tease` is the date of the next splash check, `hit` tells whether it is a splash.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Wait {
    pub left: i32,
    pub stamp: u64,
    pub speed: i32,
    pub approach: bool,
    pub tease: u64,
    pub hit: bool,
}

/// The approach is under way: the hook is visited each tick for its trail (`timeUntilHooked > 0`).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Approach;

/// The tick of the counter's end, in the hook's own time; none when the speed is 0.
fn due_of(w: &Wait) -> Option<u64> {
    ticks_at(w.left, w.speed).map(|t| w.stamp + t)
}
/// What the counter has left at `now`: the speed took off `speed` a tick since the stamp.
fn left_at(w: &Wait, now: u64) -> i32 {
    let ran = now.saturating_sub(w.stamp) as i64 * i64::from(w.speed);
    (i64::from(w.left) - ran).max(0) as i32
}
/// What the counter of the hook has left now (0 when it has none).
pub fn left_now(w: &World, e: Entity) -> i32 {
    w.get::<Wait>(e).map_or(0, |wt| left_at(wt, now_of(w, e)))
}
/// Arms the wheel at the counter's end, and the splash check of the lure.
fn arm(w: &mut World, e: Entity) {
    let Some(wt) = w.get::<Wait>(e).copied() else { return };
    let now = now_of(w, e);
    if let Some(at) = due_of(&wt) {
        let f: fn(&mut World, Entity) = if wt.approach { hooked_due } else { lured_due };
        after(w, e, (at - now) as i32, f);
    }
    if !wt.approach {
        plan_tease(w, e);
    }
}
/// Plans the next splash of the lure: a geometric draw of the vanilla chance (`lureTease` of what is left), capped where the
/// chance changes (under 60, 40, 20 ticks) or the lure ends. The date is kept in `tease`; a planned check with no splash only plans again.
fn plan_tease(w: &mut World, e: Entity) {
    let now = now_of(w, e);
    let Some(wt) = w.get::<Wait>(e).copied() else { return };
    if wt.approach || wt.speed <= 0 {
        w.entity_mut(e).insert(Wait { tease: NEVER, hit: false, ..wt });
        return;
    }
    let c = left_at(&wt, now);
    let edge = if c >= 60 { 60 } else if c >= 40 { 40 } else if c >= 20 { 20 } else { 0 };
    let cap = ((c - edge) / wt.speed + 1).min(ticks_at(c, wt.speed).unwrap_or(1) as i32).max(1) as u64;
    let Some(mut h) = w.get_mut::<Hook>(e) else { return };
    let g = geometric_after(&mut h.rng, lure_tease(c));
    drop(h);
    let at = now + g.min(cap);
    w.entity_mut(e).insert(Wait { tease: at, hit: g <= cap, ..wt });
    after(w, e, (at - now) as i32, tease_due);
}
/// A splash check of the lure is due: a splash if the draw hit, then the next check.
fn tease_due(w: &mut World, e: Entity) {
    let now = now_of(w, e);
    let Some(wt) = w.get::<Wait>(e).copied() else { return };
    if wt.approach || wt.tease != now {
        return;
    }
    w.entity_mut(e).insert(Wait { tease: NEVER, hit: false, ..wt });
    if wt.hit {
        lure_splash(w, e);
    }
    plan_tease(w, e);
}

/// A hook lands in the water: its lure's wait starts with `left` fish-ticks at `speed` (`catchingFish` with no counter).
pub fn arm_lure(w: &mut World, e: Entity, left: i32, speed: i32) {
    let now = now_of(w, e);
    w.entity_mut(e).insert(Wait { left, stamp: now, speed, approach: false, tease: NEVER, hit: false });
    index_wait(w, e);
    arm(w, e);
}

/// The lure's end: the fish is lured, its angle is drawn and its approach starts (`catchingFish` at 0 of the lure).
fn lured_due(w: &mut World, e: Entity) {
    let Some(wt) = w.get::<Wait>(e).copied() else { return };
    let now = now_of(w, e);
    if wt.approach || due_of(&wt) != Some(now) {
        return;
    }
    let Some(mut h) = w.get_mut::<Hook>(e) else { return };
    let angle = h.rng.next_float() * 360.0;
    h.fish_angle = angle;
    let left = hooked_after(&mut h.rng);
    drop(h);
    w.entity_mut(e).insert((Wait { left, stamp: now, speed: wt.speed, approach: true, tease: NEVER, hit: false }, Approach));
    arm(w, e);
}

/// The approach's end: the bite (`catchingFish` at 0 of the approach), the counter is gone.
fn hooked_due(w: &mut World, e: Entity) {
    let Some(wt) = w.get::<Wait>(e).copied() else { return };
    if !wt.approach || due_of(&wt) != Some(now_of(w, e)) {
        return;
    }
    w.entity_mut(e).remove::<(Wait, Approach)>();
    bite_due(w, e);
}

/// The fish's speed changed (rain or sky): the counter keeps what is left and runs at the new speed, its dates move.
pub fn speed_changed(w: &mut World, e: Entity, speed: i32) {
    let now = now_of(w, e);
    let Some(wt) = w.get::<Wait>(e).copied() else { return };
    if wt.speed == speed {
        return;
    }
    let left = left_at(&wt, now);
    w.entity_mut(e).insert(Wait { left, stamp: now, speed, ..wt });
    arm(w, e);
}

/// The hook leaves the water: its counter is dropped, and the deadlines still on the wheel find no matching date and do nothing.
pub fn leave_water(w: &mut World, e: Entity) {
    if let Ok(mut m) = w.get_entity_mut(e) {
        m.remove::<(Wait, Approach)>();
    }
}

/// Whether the fish is approaching (`timeUntilHooked > 0`): the hook is waiting for its bite.
pub fn approaching(w: &World, e: Entity) -> bool {
    w.get::<Wait>(e).is_some_and(|x| x.approach)
}

/// The hooks by player (the last cast) and the waiting hooks by cell of the surface. An entry is checked when read: a despawned hook is dropped.
#[derive(Resource, Default)]
pub struct HookIndex {
    pub owners: HashMap<u32, Entity>,
    pub cells: HashMap<[i32; 3], Vec<Entity>>,
}

/// Sets the waiting float at its rest, with no velocity: it is not visited between its dates.
pub fn settle(w: &mut World, e: Entity, cell_y: i32, liquid: f32) {
    let rest = crate::rules::fishing_hook::rest_height(cell_y, liquid);
    let x = w.get::<Pos>(e).map_or(0.0, |p| p.0.x);
    let z = w.get::<Pos>(e).map_or(0.0, |p| p.0.z);
    w.entity_mut(e).insert((Pos(Vec3::new(x, rest, z)), Vel(Vec3::ZERO)));
}

/// Records the hook `e` of `owner` at its cast.
pub fn own(w: &mut World, owner: u32, e: Entity) {
    w.init_resource::<HookIndex>();
    w.resource_mut::<HookIndex>().owners.insert(owner, e);
}

/// Records the waiting hook `e` at its cell (a hook in the water does not move).
fn index_wait(w: &mut World, e: Entity) {
    let Some(key) = w.get::<Pos>(e).map(|p| crate::fishing_hook::cell(p.0)) else { return };
    w.init_resource::<HookIndex>();
    let mut index = w.resource_mut::<HookIndex>();
    let list = index.cells.entry(key).or_default();
    if !list.contains(&e) {
        list.push(e);
    }
}

/// The waiting hooks of the cell `key`, the ones still waiting (the others are dropped).
pub fn waiting_at(w: &mut World, key: [i32; 3]) -> Vec<Entity> {
    let list = w.get_resource_mut::<HookIndex>().and_then(|mut x| x.cells.remove(&key)).unwrap_or_default();
    let live: Vec<Entity> = list.into_iter().filter(|&e| w.get::<Wait>(e).is_some()).collect();
    if !live.is_empty() {
        w.resource_mut::<HookIndex>().cells.insert(key, live.clone());
    }
    live
}

/// The owner's waiting hook is removed when its player is gone, has no rod in hand or is over 32 blocks away
/// (`FishingHook.shouldStopFishing`), at the player's change or removal: no walk of the hooks.
pub fn owner_changed(w: &mut World, owner: u32, player: Option<Player>) {
    let Some(e) = w.get_resource::<HookIndex>().and_then(|x| x.owners.get(&owner).copied()) else { return };
    if w.get::<Hook>(e).is_none_or(|h| h.owner != owner) || w.get::<Wait>(e).is_none() {
        return;
    }
    let stop = player.is_none_or(|p| {
        let rod = item_id("minecraft:fishing_rod");
        let q = w.get::<Pos>(e).map_or([0.0; 3], |q| [q.0.x, q.0.y, q.0.z]);
        let (dx, dy, dz) = (q[0] - p.x, q[1] - p.y, q[2] - p.z);
        !p.holds(rod) || dx * dx + dy * dy + dz * dz > 1024.0
    });
    if stop {
        leave_water(w, e);
        w.despawn(e);
    }
}
