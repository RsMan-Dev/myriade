//! Minecarts on rails (`OldMinecartBehavior.tick`, `moveAlongTrack`, `comeOffTrack`) for the carts that roll: a cart with `Rolling` has a speed,
//! a rider or a lit powered rail under it. A cart at rest on a plain rail has no `Rolling` and costs nothing; a push, a rider, a blow or a
//! changed rail puts it back. The physics are the pure functions of `minecart` and `rules::minecart`; the host's reads of the track (the rail
//! shape with its mod hooks, the conductors, the activator rails, the container signal) come in through `TrackRead`.
use crate::block_factor::{self, BlockSpeed};
use crate::cart_state::{Cart, Contents};
use crate::cow::{block_changes, Index, Level, Pos, Vel};
use crate::goals::sparse_any;
use crate::host_out_cart_move::{CartMoveEvent, CartMoveOut};
use crate::minecart::{along_track, can_park, container_slowdown, fall_step, furnace_slowdown, off_track_drag, off_track_velocity, rail_point, settle, slowdown, Fall, Furnace, Kind, Rail, Track, GRAVITY, MAX_LAND, MAX_WATER};
use crate::seat;
use bevy_ecs::{entity_disabling::Disabled, prelude::*};
use horizon_client_lib::items::ItemStack;
use horizon_client_lib::mobs::MobWorld;
use horizon_core::geometry::Vec3;

/// A cart that moves, has a rider or sits on a lit powered rail: `cart_step` visits only these.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
#[component(storage = "SparseSet")]
pub struct Rolling;

/// The host's reads of the track, lent once to the world: the rail under a cell, whether a cell is a redstone conductor, whether a rail
/// is an activator, and the comparator signal of a container's slots. The rail shape's mod hooks stay on the host's side.
#[derive(Resource, Clone, Copy)]
pub struct TrackRead {
    pub rail: fn(&MobWorld<'static>, [i32; 3]) -> Option<Rail>,
    pub conductor: fn(&MobWorld<'static>, [i32; 3]) -> bool,
    pub activator: fn(&MobWorld<'static>, [i32; 3]) -> bool,
    pub signal: fn(&MobWorld<'static>, &[Option<ItemStack>]) -> i32,
}

/// Wakes a cart: it joins the rolling carts until it parks again.
pub fn wake(w: &mut World, e: Entity) {
    if w.get::<Cart>(e).is_some() {
        w.entity_mut(e).insert(Rolling);
    }
}

/// Adds `impulse` to the cart's velocity and wakes it. Nothing for an entity that is not a cart.
pub fn push(w: &mut World, e: Entity, impulse: [f64; 3]) {
    let Some(mut v) = w.get_mut::<Vel>(e) else { return };
    v.0 = Vec3::new(v.0.x + impulse[0], v.0.y + impulse[1], v.0.z + impulse[2]);
    drop(v);
    wake(w, e);
}

/// A rail at `pos` changed or got its power: the resting carts near it wake. The creature index answers the column search; no cart list is
/// scanned.
pub fn rail_changed(w: &mut World, pos: [i32; 3]) {
    let (x, z) = (f64::from(pos[0]) + 0.5, f64::from(pos[2]) + 0.5);
    let Some(index) = w.get_resource::<Index>() else { return };
    let mut near = Vec::new();
    index.for_radius(x, z, 1.0, |e| near.push(e));
    for e in near {
        let disabled = w.get_entity(e).is_ok_and(|r| r.contains::<Disabled>());
        if !disabled && w.get::<Rolling>(e).is_none() {
            wake(w, e);
        }
    }
}

/// The velocity of a cart, None for an entity that is not one.
pub fn velocity(w: &World, e: Entity) -> Option<[f64; 3]> {
    w.get::<Cart>(e)?;
    let v = w.get::<Vel>(e)?.0;
    Some([v.x, v.y, v.z])
}

/// The events since the last drain, in order.
pub fn drain_events(w: &mut World) -> Vec<CartMoveEvent> {
    w.get_resource_mut::<CartMoveOut>().map(|mut o| std::mem::take(&mut o.0)).unwrap_or_default()
}

/// The rail a cart at `p` runs on: the rail one block below when there is one, else the rail in its own block (`rail_block`).
pub(crate) fn rail_under(read: TrackRead, mw: &MobWorld<'static>, p: Vec3) -> Option<([i32; 3], Rail)> {
    let (x, y, z) = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
    if let Some(r) = (read.rail)(mw, [x, y - 1, z]) {
        return Some(([x, y - 1, z], r));
    }
    (read.rail)(mw, [x, y, z]).map(|r| ([x, y, z], r))
}

/// One tick of every rolling cart: on a rail it moves along the line, else it falls and slides off it.
fn cart_step(world: &mut World, mut cached: Local<Option<QueryState<Entity, With<Rolling>>>>) {
    let (Some(level), Some(read)) = (world.get_resource::<Level>().copied(), world.get_resource::<TrackRead>().copied()) else { return };
    world.init_resource::<CartMoveOut>();
    let state = cached.get_or_insert_with(|| world.query_filtered());
    let list: Vec<Entity> = state.iter(world).collect();
    let mw = level.mw.0;
    for e in list {
        step(world, e, mw, read);
    }
}

fn step(w: &mut World, e: Entity, mw: &MobWorld<'static>, read: TrackRead) {
    let (Some(kind), Some(t), Some(p), Some(v)) = (
        w.get::<Kind>(e).copied(),
        w.get::<Track>(e).copied(),
        w.get::<Pos>(e).map(|p| p.0),
        w.get::<Vel>(e).map(|v| v.0),
    ) else {
        return;
    };
    let ridden = !seat::riders_of(w, e).is_empty() || !seat::players_of(w, e).is_empty();
    let push = w.get::<Furnace>(e).map_or(Vec3::ZERO, |f| f.push);
    let mut fall = w.get::<Fall>(e).map_or(0.0, |f| f.0);
    let mut vel = Vec3::new(v.x, v.y - GRAVITY, v.z);
    let found = rail_under(read, mw, p);
    let old_y = found.as_ref().map(|(c, r)| rail_point(r, *c, p).y);
    let mut pos = p;
    let (on_ground, rail) = match found {
        Some((cell, rail)) => {
            let intent = w.get::<crate::pilot::PilotInput>(e).map_or([0.0; 2], crate::pilot::cart_intent);
            let delta = along_track(t, &mut pos, &mut vel, cell, &rail, intent, ridden);
            pos.x += delta.x;
            pos.z += delta.z;
            fall_step(&mut fall, pos.y - p.y, t.in_water, true, kind == Kind::Tnt);
            let (slowed, push) = if kind == Kind::Furnace {
                furnace_slowdown(vel, push, t.in_water)
            } else if let Some(c) = w.get::<Contents>(e) {
                let signal = (read.signal)(mw, &c.slots);
                (container_slowdown(vel, signal, c.loot.is_some(), t.in_water), push)
            } else {
                (slowdown(vel, ridden, t.in_water), push)
            };
            let new_y = rail_under(read, mw, pos).map(|(c, r)| rail_point(&r, c, pos).y);
            let c = |dx: i32, dz: i32| (read.conductor)(mw, [cell[0] + dx, cell[1], cell[2] + dz]);
            let conductors = match rail.powered {
                Some(true) if rail.exits[0][0] != rail.exits[1][0] => (c(-1, 0), c(1, 0)),
                Some(true) => (c(0, -1), c(0, 1)),
                _ => (false, false),
            };
            settle(&rail, cell, &mut pos, &mut vel, slowed, old_y, new_y, conductors);
            if rail.powered == Some(true) {
                emit(w, CartMoveEvent::PoweredBoost { entity: e });
            }
            if rail.powered == Some(true) && (read.activator)(mw, cell) && kind == Kind::Rideable {
                seat::eject_all(w, e);
            }
            (true, Some((rail, push)))
        }
        None => {
            let max = if t.in_water { MAX_WATER } else { MAX_LAND };
            vel = off_track_velocity(vel, t.on_ground, max);
            if vel.horizontal().length_squared() > 0.0 {
                emit(w, CartMoveEvent::Derail { entity: e });
            }
            pos = Vec3::new(pos.x + vel.x, pos.y + vel.y, pos.z + vel.z);
            let on_ground = t.on_ground;
            fall_step(&mut fall, pos.y - p.y, t.in_water, on_ground, kind == Kind::Tnt);
            if on_ground {
                vel.y = 0.0;
            }
            vel = off_track_drag(vel, on_ground);
            (on_ground, None)
        }
    };
    // `Entity.getBlockSpeedFactor` is 1.0 on a rail cell (the cart's own cell, as `AbstractMinecart` reads it), else the block's factor.
    if w.get::<BlockSpeed>(e).is_some() && (read.rail)(mw, [pos.x.floor() as i32, pos.y.floor() as i32, pos.z.floor() as i32]).is_none() {
        let f = f64::from(block_factor::speed(mw, [pos.x, pos.y, pos.z]));
        vel = Vec3::new(vel.x * f, vel.y, vel.z * f);
    }
    if let Some(mut q) = w.get_mut::<Pos>(e) {
        q.0 = pos;
    }
    if let Some(mut q) = w.get_mut::<Vel>(e) {
        q.0 = vel;
    }
    if let Some(mut q) = w.get_mut::<Track>(e) {
        q.on_ground = on_ground;
    }
    if fall != 0.0 {
        w.entity_mut(e).insert(Fall(fall));
    } else {
        w.entity_mut(e).remove::<Fall>();
    }
    if let Some(mut f) = w.get_mut::<Furnace>(e) {
        f.push = rail.as_ref().map_or(push, |(_, kept)| *kept);
    }
    let parked = match &rail {
        Some((r, push)) => can_park(Some(r), vel, ridden, *push),
        None => on_ground && vel.length_squared() == 0.0,
    };
    if parked {
        w.entity_mut(e).remove::<Rolling>();
    }
    crate::vehicle_push::push_pairs(w, e);
}

fn emit(w: &mut World, ev: CartMoveEvent) {
    w.init_resource::<CartMoveOut>();
    w.resource_mut::<CartMoveOut>().0.push(ev);
}

pub fn register(s: &mut bevy_ecs::schedule::Schedule) {
    s.add_systems(cart_step.run_if(|w: &bevy_ecs::world::World| sparse_any::<Rolling>(w)).after(block_changes));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cart(w: &mut World, v: Vec3) -> Entity {
        w.spawn((Cart, Kind::Rideable, Track::default(), Pos(Vec3::ZERO), Vel(v))).id()
    }

    #[test]
    fn a_resting_cart_is_not_rolling() {
        let mut w = World::new();
        let e = cart(&mut w, Vec3::ZERO);
        assert!(w.get::<Rolling>(e).is_none());
        assert!(!sparse_any::<Rolling>(&w));
    }

    #[test]
    fn a_push_wakes_the_cart_and_adds_the_impulse() {
        let mut w = World::new();
        let e = cart(&mut w, Vec3::ZERO);
        push(&mut w, e, [0.1, 0.0, -0.2]);
        assert!(w.get::<Rolling>(e).is_some());
        assert_eq!(velocity(&w, e), Some([0.1, 0.0, -0.2]));
        assert!(sparse_any::<Rolling>(&w));
    }

    #[test]
    fn a_push_of_an_entity_that_is_not_a_cart_does_nothing() {
        let mut w = World::new();
        let e = w.spawn((Kind::Rideable, Pos(Vec3::ZERO), Vel(Vec3::ZERO))).id();
        push(&mut w, e, [1.0, 0.0, 0.0]);
        assert!(w.get::<Rolling>(e).is_none() && velocity(&w, e).is_none());
    }
}
