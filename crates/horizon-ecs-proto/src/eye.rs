//! The thrown eye of ender (`EyeOfEnder`, Horizon `game/world/end.rs` `EyeOfEnder` and `tick_eyes`). It flies towards its target
//! every tick of its own life, then shatters at its 81st tick (`life > 80`), dropping its item when it survives. The shatter is
//! one wake-up on the wheel (`after`), not a count: the flight is the eye's only per-tick work.
use crate::cow::{after, OwnTime, Pos, Vel};
use crate::goals::Events;
use crate::item::drop_item;
use crate::rules::eye::update_delta;
use crate::cow_helpers::with_level_random;
use bevy_ecs::prelude::*;
use myriade_engine_lib::items::ItemStack;
use horizon_core::geometry::Vec3;
use horizon_core::random::RandomSource;

/// The tick at which the eye shatters (`EyeOfEnder.tick`: `life > 80`).
const SHATTER_AT: i32 = 81;

/// The eye's flight: where it goes and whether it survives its end (`surviveAfterDeath`, one chance in five of shattering).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Eye {
    pub target: Vec3,
    pub survive: bool,
    /// `signalTo` ran: no target (a summoned eye) leaves the velocity as it is (`EyeOfEnder.tick`: `target != null`).
    pub aimed: bool,
}

/// Throws an eye at `at` towards `target`, its velocity nil (`signalTo` ran); it shatters on its 81st tick.
pub fn launch(w: &mut World, at: Vec3, target: Vec3, survive: bool) -> Entity {
    let e = w.spawn((Pos(at), Vel(Vec3::ZERO), Eye { target, survive, aimed: true }, OwnTime)).id();
    after(w, e, SHATTER_AT, shatter);
    e
}

/// The flight, every tick: the eye moves by its velocity, then the velocity turns towards the target (`EyeOfEnder.tick`, the
/// move before the update of the velocity is the vanilla order: `position + deltaMovement`, then `updateDeltaMovement`).
pub fn fly(mut q: Query<(Entity, &mut Pos, &mut Vel, &Eye)>, mut sent: Option<ResMut<crate::bridge::view::Modified>>) {
    for (e, mut pos, mut vel, eye) in &mut q {
        let next = pos.0.add_vector(vel.0);
        if eye.aimed {
            vel.0 = update_delta(vel.0, next, eye.target);
        }
        pos.0 = next;
        // A tracked eye is sent again where it moved (the tracker reads `Modified`).
        if let Some(m) = sent.as_mut() {
            m.push_motion(e);
        }
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
    if !eye.survive {
        // `Level.levelEvent(2003, blockPosition(), 0)`: the crumbs and the portal rings.
        w.resource_mut::<Events>().host.level_event(2003, [at.x.floor() as i32, at.y.floor() as i32, at.z.floor() as i32], 0);
    }
    if eye.survive {
        let (vx, vz) = with_level_random(w, |r| (r.next_double() * 0.2 - 0.1, r.next_double() * 0.2 - 0.1));
        drop_item(w, ItemStack::new("minecraft:ender_eye", 1), at, Vec3::new(vx, 0.2, vz), 0);
    }
}

/// An eye thrown by a player (`EyeOfEnder.signalTo` ran): saved and typed like a summoned one.
pub fn throw(w: &mut World, at: Vec3, target: Vec3, survive: bool) -> Entity {
    let e = launch(w, at, target, survive);
    w.entity_mut(e).insert(crate::cow::TypeName(ID));
    crate::nbt::mark_saved(w, e, ID);
    e
}

/// An eye made by `summon`: no `signalTo` ran, so it has no target (`target` is null) and it does not survive (`surviveAfterDeath` false).
pub fn build(w: &mut World, at: Vec3, _: &horizon_core::nbt::Compound) -> Option<Entity> {
    let e = launch(w, at, Vec3::ZERO, false);
    w.get_mut::<Eye>(e)?.aimed = false;
    w.entity_mut(e).insert(crate::cow::TypeName(ID));
    crate::nbt::mark_saved(w, e, ID);
    Some(e)
}
pub const ID: &str = "minecraft:eye_of_ender";

pub fn register(s: &mut bevy_ecs::schedule::Schedule) {
    s.add_systems(fly.run_if(|w: &World| crate::goals::sparse_any::<Eye>(w)));
}
