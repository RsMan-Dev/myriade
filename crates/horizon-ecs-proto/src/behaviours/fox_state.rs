//! The state the three fox goals share (fiches 21 to 23): the flags of `Fox` (`fox.rs:37-43`), the crouch clock that stands in
//! for the crouch amount of the animation (`fox.rs:351-361`), the deadline a refused `canUse` armed, and the geometry the
//! goals read (`path_clear`, `fox.rs:1094-1121`). Not a module: nothing here is in `TABLE`.
use crate::cow::*;
use crate::goals::*;
use crate::rules::fox_state::{crouch_ticks_to_full, is_selector_tick, retry_wait};
use super::trust::Trust;
use crate::targeting::{self, Info, Target, Who};
use bevy_ecs::prelude::*;
use horizon_core::geometry::Vec3;

pub const SITTING: u8 = 1;
pub const CROUCHING: u8 = 4;
pub const INTERESTED: u8 = 8;
pub const POUNCING: u8 = 16;
pub const SLEEPING: u8 = 32;
pub const FACEPLANTED: u8 = 64;

/// `Fox.DATA_FLAGS_ID`: the flags the goals set and read.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct FoxFlags(pub u8);

/// The local time at which CROUCHING was set (None: not crouching).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Crouch(pub Option<u64>);

/// The local time of the deadline a refused `canUse` armed, None when none is pending.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Retry(pub Option<u64>);

/// Creates the shared components of the fox, once (each goal's `equip` calls it). The trust is created even when a baby already got one.
pub fn ensure(w: &mut World, e: Entity) {
    if w.get::<Trust>(e).is_none() {
        w.entity_mut(e).insert(Trust::default());
    }
    if w.get::<FoxFlags>(e).is_none() {
        w.entity_mut(e).insert((FoxFlags::default(), Crouch::default(), Retry::default()));
    }
}

/// Any of the flags of the mask is set.
pub fn has(w: &World, e: Entity, mask: u8) -> bool {
    w.get::<FoxFlags>(e).is_some_and(|f| f.0 & mask != 0)
}

/// Sets or clears the flags of the mask; CROUCHING starts (or ends) the crouch clock.
pub fn set(w: &mut World, e: Entity, mask: u8, on: bool) {
    let now = now_of(w, e);
    let Some(mut f) = w.get_mut::<FoxFlags>(e) else { return };
    let was = f.0 & CROUCHING != 0;
    if on {
        f.0 |= mask;
    } else {
        f.0 &= !mask;
    }
    let is = f.0 & CROUCHING != 0;
    drop(f);
    if !on && mask & (SLEEPING | SITTING | FACEPLANTED) != 0 {
        super::fox_search_for_items::flags_cleared(w, e);
    }
    if let Some(mut c) = w.get_mut::<Crouch>(e) {
        if !is {
            c.0 = None;
        } else if !was {
            c.0 = Some(now);
        }
    }
}

/// The local time from which the fox is fully crouched, if it is crouching.
pub fn fully_crouched_at(w: &World, e: Entity) -> Option<u64> {
    w.get::<Crouch>(e)?.0.map(|t| t + crouch_ticks_to_full())
}

/// A tick where the `Mob`'s selector asks `canUse` and ticks the goal (`ai.rs:2870`).
pub fn selector_tick(w: &World, e: Entity) -> bool {
    is_selector_tick(now_of(w, e), phase_of(w, e))
}

/// Arms the deadline that asks `canUse` again, `wait` ticks from now, unless one is still pending (a refusal that repeats
/// must not stack deadlines).
pub fn retry_in(w: &mut World, e: Entity, slot: u8, wait: i32) {
    let now = now_of(w, e);
    let wait = retry_wait(wait);
    let Some(mut r) = w.get_mut::<Retry>(e) else { return };
    if r.0.is_some_and(|t| t > now) {
        return;
    }
    r.0 = Some(now + wait);
    drop(r);
    arm(w, e, slot, wait as i32);
}

/// The deadline of `retry_in` has come: nothing is pending any more.
pub fn retry_done(w: &mut World, e: Entity) {
    if let Some(mut r) = w.get_mut::<Retry>(e) {
        r.0 = None;
    }
}

/// The current target of the attacker when it is alive: a player who is alive (`targeting::info` reads that), or a
/// creature with health.
pub fn live_target(w: &World, e: Entity) -> Option<(Who, Info)> {
    let who = w.get::<Target>(e)?.who;
    let info = targeting::info(w, who)?;
    if let Who::Entity(c) = who
        && !w.get::<Health>(c).is_some_and(|h| h.0 > 0.0)
    {
        return None;
    }
    Some((who, info))
}

/// The target is a creature of one of the named types (`TypeName`); a player is never one.
pub fn is_type(w: &World, who: Who, types: &[&str]) -> bool {
    match who {
        Who::Entity(c) => w.get::<TypeName>(c).is_some_and(|n| types.contains(&n.0)),
        Who::Player(_) => false,
    }
}

/// The feet of the entity.
pub fn feet(w: &World, e: Entity) -> Vec3 {
    w.get::<Pos>(e).unwrap().0
}

/// `Fox.isPathClear`: six points on the way to `to`, each with only replaceable blocks in the three above it.
pub fn path_clear(w: &World, from: Vec3, to: Vec3) -> bool {
    crate::rules::fox_state::path_clear(w.resource::<Level>().mw.0, from, to)
}

/// The block the feet stand in is snow (`Fox.FoxPounceGoal`: the nose lands in the snow).
pub fn in_snow(w: &World, p: Vec3) -> bool {
    let mw = w.resource::<Level>().mw.0;
    mw.block_name(p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32) == "minecraft:snow"
}

/// Asks the path to `at` at `speed`, to reach it exactly (reach 0). The `go_to` of the goals (`goals.rs:880`) without its
/// statistics (its array holds six goals), with reach 0 instead of 1; `stop_walk` gives the reach back.
pub fn walk_to(w: &mut World, e: Entity, at: Vec3, speed: f64) {
    crate::goals::go_to_at(w, e, [at.x, at.y, at.z], speed, 0);
}

/// Gives back the reach of `walk_to`.
pub fn stop_walk(w: &mut World, e: Entity) {
    stop_nav(w, e);
}

/// `m.look_at_speed`: the head turns to `at` at `y_speed` (the default when None). The `aim` of the goals, which takes
/// `Commands` where this one has the world.
pub fn aim_at(w: &mut World, e: Entity, at: [f64; 3], y_speed: Option<f32>) {
    let looking = w.get::<Looking>(e).is_some();
    if let Some(mut l) = w.get_mut::<Look>(e) {
        l.target = at;
        l.cooldown = 2;
        l.y_speed = y_speed.unwrap_or(l.y_default);
    }
    if !looking {
        w.entity_mut(e).insert(Looking);
    }
}

/// The entities that carry the marker `T` (a goal that runs), as a list: empty costs nothing.
pub fn marked<T: Component>(w: &mut World) -> Vec<Entity> {
    crate::queries::entities::<With<T>>(w)
}
