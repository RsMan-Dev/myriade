//! The thrown eye of ender (`EyeOfEnder`, Horizon `game/world/end.rs` `EyeOfEnder` and `tick_eyes`). It flies towards its target
//! every tick of its own life, then shatters at its 81st tick (`life > 80`), dropping its item when it survives. The shatter is
//! one wake-up on the wheel (`after`), not a count: the flight is the eye's only per-tick work.
use crate::cow::{after, OwnTime, Pos, Vel};
use crate::goals::Events;
use crate::item::drop_item;
use crate::cow_helpers::with_level_random;
use bevy_ecs::prelude::*;
use horizon_client_lib::items::ItemStack;
use horizon_core::geometry::Vec3;
use horizon_core::random::RandomSource;

/// The tick at which the eye shatters (`EyeOfEnder.tick`: `life > 80`).
const SHATTER_AT: i32 = 81;

/// The eye's flight: where it goes and whether it survives its end (`surviveAfterDeath`, one chance in five of shattering).
#[derive(Component, Clone, Copy, Debug)]
pub struct Eye {
    pub target: Vec3,
    pub survive: bool,
}

/// `EyeOfEnder.updateDeltaMovement`: the horizontal speed closes 0.25 % of its gap to the target per tick, slower under a block
/// away; the vertical speed drifts 1.5 % towards up or down, by whether the eye is below the target's height.
pub fn update_delta(old: Vec3, pos: Vec3, target: Vec3) -> Vec3 {
    let (hx, hz) = (target.x - pos.x, target.z - pos.z);
    let length = hx.hypot(hz);
    let old_h = old.x.hypot(old.z);
    let mut speed = old_h + 0.0025 * (length - old_h);
    let mut y = old.y;
    if length < 1.0 {
        speed *= 0.8;
        y *= 0.8;
    }
    let wanted = if pos.y - old.y < target.y { 1.0 } else { -1.0 };
    Vec3::new(hx * (speed / length), y + (wanted - y) * 0.015, hz * (speed / length))
}

/// Throws an eye at `at` towards `target`, its velocity nil (`signalTo` ran); it shatters on its 81st tick.
pub fn launch(w: &mut World, at: Vec3, target: Vec3, survive: bool) -> Entity {
    let e = w.spawn((Pos(at), Vel(Vec3::ZERO), Eye { target, survive }, OwnTime)).id();
    after(w, e, SHATTER_AT, shatter);
    e
}

/// The flight, every tick: the eye moves by its velocity, then the velocity turns towards the target (`EyeOfEnder.tick`, the
/// move before the update of the velocity is the vanilla order: `position + deltaMovement`, then `updateDeltaMovement`).
pub fn fly(mut q: Query<(&mut Pos, &mut Vel, &Eye)>) {
    for (mut pos, mut vel, eye) in &mut q {
        let next = pos.0.add_vector(vel.0);
        vel.0 = update_delta(vel.0, next, eye.target);
        pos.0 = next;
    }
}

/// The shatter (`EyeOfEnder.tick` at `life > 80`): the death sound; the item drops with a random spread of 0.2 on x and z when the
/// eye survives, else the level event 2003 (the crumbs and portal rings, a host output not yet in `HostEvent`).
fn shatter(w: &mut World, e: Entity) {
    let Some(eye) = w.get::<Eye>(e).copied() else { return };
    let at = w.get::<Pos>(e).map_or(Vec3::ZERO, |p| p.0);
    w.despawn(e);
    let at_arr = [at.x, at.y, at.z];
    w.resource_mut::<Events>().host.sound("minecraft:entity.ender_eye.death", at_arr, 1.0, 1.0);
    if eye.survive {
        let (vx, vz) = with_level_random(w, |r| (r.next_double() * 0.2 - 0.1, r.next_double() * 0.2 - 0.1));
        drop_item(w, ItemStack::new("minecraft:ender_eye", 1), at, Vec3::new(vx, 0.2, vz), 0);
    }
}

pub fn register(s: &mut bevy_ecs::schedule::Schedule) {
    s.add_systems(fly);
}
