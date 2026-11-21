//! The pushes between vehicles and the entities their boxes touch (`AbstractBoat.tick`'s end, `Entity.push`, the cart's push of a creature).
//! The neighbours come from the spatial index, never from a scan of the world; the rules are in `rules::push_rules`. A pushed creature is
//! woken, gets its impulse as `PushIn` and `Moving`; a boat takes it in its velocity. A player is not moved here: the push of a player is an
//! event for the host (`PushPlayer`), found through `Online`. A cart against a cart or a boat follows `AbstractMinecart.push`.
use crate::boat_state::BoatMarker;
use crate::cart_move;
use crate::cart_state::Cart;
use crate::cow::*;
use crate::goals::Online;
use crate::hostile::PlayerState;
use crate::host_out_vehicle_push::{VehiclePushEvent, VehiclePushEvents};
use crate::minecart::Furnace;
use crate::rules::area::centre_radius;
use crate::rules::minecart::push_from;
use crate::rules::push_rules::{self as rules, push_impulse, TOUCH_REACH};
use crate::seat::{Seated, Seats};
use crate::targeting::{player_box, Who};
use crate::zone::{wake, Dormant};
use bevy_ecs::prelude::*;
use horizon_core::geometry::{Aabb, Vec3};

/// The reusable buffers of `push_pairs` (the neighbours, the players), kept in the world so a step allocates nothing once they have grown.
#[derive(Resource, Default)]
struct PushBuf {
    contacts: Vec<Entity>,
    players: Vec<u32>,
}

fn bounds_of(w: &World, e: Entity) -> Option<Aabb> {
    w.get::<Bounds>(e).map(|b| b.0)
}

fn pos_of(w: &World, e: Entity) -> Option<Vec3> {
    w.get::<Pos>(e).map(|p| p.0)
}

fn is_boat(w: &World, e: Entity) -> bool {
    w.get::<BoatMarker>(e).is_some()
}

fn is_cart(w: &World, e: Entity) -> bool {
    w.get::<Cart>(e).is_some()
}

fn is_vehicle(w: &World, e: Entity) -> bool {
    is_boat(w, e) || is_cart(w, e)
}

/// `Entity.isPushable` and `moved_by_push`: an object (`PushFree`) is not pushed, nor is a rider, nor a vehicle that carries riders.
pub fn pushable(w: &World, e: Entity) -> bool {
    w.get::<PushFree>(e).is_none() && w.get::<Seated>(e).is_none() && !w.get::<Seats>(e).is_some_and(|s| s.riders.iter().any(Option::is_some))
}

/// Adds `impulse` to the velocity of `e` and wakes it: a sleeping creature first, a cart by `cart_move::push`, a boat in its velocity, any
/// other creature by its pending push (`add_push`).
pub fn push(w: &mut World, e: Entity, impulse: [f64; 3]) {
    let now = w.resource::<Clock>().0;
    if w.get::<Dormant>(e).is_some() {
        wake(w, e, now);
    }
    if is_cart(w, e) {
        cart_move::push(w, e, impulse);
    } else if is_boat(w, e) {
        if let Some(mut v) = w.get_mut::<Vel>(e) {
            v.0 = Vec3::new(v.0.x + impulse[0], v.0.y + impulse[1], v.0.z + impulse[2]);
        }
        w.entity_mut(e).insert(Moving);
    } else {
        add_push(w, e, Vec3::new(impulse[0], impulse[1], impulse[2]), true);
    }
}

/// The entities whose box touches the box of `vehicle` grown by `extra` on x and z, and by -0.01 on y as `AbstractBoat` does: the index's
/// columns around it, then the real boxes. The vehicle itself and its own riders are left out. Read only; `out` is cleared and filled.
pub fn contacts(w: &World, vehicle: Entity, extra: f64, out: &mut Vec<Entity>) {
    out.clear();
    let Some(bb) = bounds_of(w, vehicle) else { return };
    let area = bb.inflate(Vec3::new(extra, -0.01, extra));
    let (mn, mx, c) = (area.min_position(), area.max_position(), bb.center());
    w.resource::<Index>().for_near_touching(c.x, c.z, (mn.x, mn.z), (mx.x, mx.z), |e| out.push(e));
    out.retain(|&e| e != vehicle && w.get::<Seated>(e).map(|s| s.vehicle) != Some(vehicle) && bounds_of(w, e).is_some_and(|b| b.intersects(area)));
}

fn emit(w: &mut World, ev: VehiclePushEvent) {
    w.init_resource::<VehiclePushEvents>();
    w.resource_mut::<VehiclePushEvents>().0.push(ev);
}

fn vel_of(w: &World, e: Entity) -> Vec3 {
    w.get::<Vel>(e).map_or(Vec3::ZERO, |v| v.0)
}

/// Multiplies the horizontal velocity of `e` by `k`, its vertical one kept (`multiply(k, 1, k)`).
fn scale_xz(w: &mut World, e: Entity, k: f64) {
    if let Some(mut v) = w.get_mut::<Vel>(e) {
        v.0 = Vec3::new(v.0.x * k, v.0.y, v.0.z * k);
    }
}

/// `AbstractMinecart.push` of a cart on another cart (`pushOtherMinecart`): nothing unless the carts face each other, |dot| at least 0.8. A
/// furnace takes the other's way, the other cart gets the rest, both keep 0.95 of their speed on x and z.
fn push_carts(w: &mut World, a: Entity, b: Entity) -> bool {
    let (Some(pa), Some(pb)) = (pos_of(w, a), pos_of(w, b)) else { return false };
    let Some([dx, dz]) = push_from(pb, pa) else { return false };
    if !pushable(w, b) {
        return false;
    }
    let yaw = w.get::<Yaw>(a).map_or(0.0, |y| f64::from(y.0)).to_radians();
    let (ux, uz) = (pb.x - pa.x, pb.z - pa.z);
    let dot = (ux * yaw.cos() + uz * yaw.sin()) / (ux * ux + uz * uz).sqrt();
    if dot.abs() < 0.8 {
        return false;
    }
    let (ta, tb) = (vel_of(w, a), vel_of(w, b));
    let (fa, fb) = (w.get::<Furnace>(a).is_some(), w.get::<Furnace>(b).is_some());
    if fb && !fa {
        scale_xz(w, a, 0.2);
        push(w, a, [tb.x - ta.x, 0.0, tb.z - ta.z]);
        scale_xz(w, b, 0.95);
    } else if fa && !fb {
        scale_xz(w, b, 0.2);
        push(w, b, [dx + ta.x, 0.0, dz + ta.z]);
        scale_xz(w, a, 0.95);
    } else {
        let (ax, az) = ((tb.x + ta.x) / 2.0, (tb.z + ta.z) / 2.0);
        scale_xz(w, a, 0.2);
        push(w, a, [ax - dx, 0.0, az - dz]);
        scale_xz(w, b, 0.2);
        push(w, b, [ax + dx, 0.0, az + dz]);
    }
    true
}

/// `AbstractMinecart.push` of a boat (not a cart): the cart takes the whole push away from the boat, the boat a quarter of it.
fn push_cart_boat(w: &mut World, cart: Entity, boat: Entity) -> bool {
    let (Some(pc), Some(pb)) = (pos_of(w, cart), pos_of(w, boat)) else { return false };
    if !pushable(w, boat) {
        return false;
    }
    let Some([dx, dz]) = push_from(pb, pc) else { return false };
    push(w, cart, [-dx, 0.0, -dz]);
    push(w, boat, [dx / 4.0, 0.0, dz / 4.0]);
    true
}

/// `Entity.push` between `a` and `b`, one way each: the second is pushed by the first and the first by the second, a boat only pushing
/// what `boat_pushes` allows, a cart pushing a creature as `AbstractMinecart.push` does. Returns whether something was pushed.
fn push_each_way(w: &mut World, a: Entity, b: Entity) -> bool {
    let mut bumped = false;
    for (x, y) in [(a, b), (b, a)] {
        let (Some(bx), Some(by), Some(px), Some(py)) = (bounds_of(w, x), bounds_of(w, y), pos_of(w, x), pos_of(w, y)) else { return bumped };
        if !pushable(w, y) || (is_boat(w, x) && !rules::boat_pushes(bx, by, is_boat(w, y))) {
            continue;
        }
        let impulse = if is_cart(w, x) && !is_vehicle(w, y) {
            push_from(py, px).map(|[p, q]| {
                push(w, x, [-p, 0.0, -q]);
                [p / 4.0, 0.0, q / 4.0]
            })
        } else if is_cart(w, x) || is_cart(w, y) {
            None
        } else {
            push_impulse(px, py).map(|v| [v.x, v.y, v.z])
        };
        if let Some(i) = impulse {
            push(w, y, i);
            bumped = true;
        }
    }
    bumped
}

/// The pushes between `a` and `b`: vanilla's bump only moves the two apart, which these writes do; only a push of a player is said to the host.
pub fn push_between(w: &mut World, a: Entity, b: Entity) {
    match (is_cart(w, a), is_cart(w, b)) {
        (true, true) => push_carts(w, a, b),
        (true, false) if is_boat(w, b) => push_cart_boat(w, a, b),
        (false, true) if is_boat(w, a) => push_cart_boat(w, b, a),
        _ => push_each_way(w, a, b),
    };
}

/// The living players whose box meets `area`, into `out` (cleared first): `area::players_in`, with the buffer.
fn players_into(w: &World, area: Aabb, out: &mut Vec<u32>) {
    out.clear();
    let (on, ps) = (w.resource::<Online>(), w.resource::<PlayerState>());
    let (c, r) = centre_radius(area);
    on.for_near(c.0, c.1, r, |p| {
        if ps.alive(p.id) && player_box(&p).intersects(area) {
            out.push(p.id);
        }
    });
}

/// The players whose box meets the vehicle's box grown as `contacts` grows it. Each one pushed by a cart or a boat gets a `PushPlayer` event
/// with the impulse a creature at its place would get (the cart takes the negation); the host applies it. Every player found gets its own event.
fn push_players(w: &mut World, vehicle: Entity, found: &mut Vec<u32>) {
    let (Some(bb), Some(vp)) = (bounds_of(w, vehicle), pos_of(w, vehicle)) else { return };
    let area = bb.inflate(Vec3::new(TOUCH_REACH, -0.01, TOUCH_REACH));
    players_into(w, area, found);
    for &id in found.iter() {
        let Some(p) = w.resource::<Online>().get(id) else { continue };
        let feet = Vec3::new(p.x, p.y, p.z);
        let impulse = if is_cart(w, vehicle) {
            push_from(feet, vp).map(|[x, z]| {
                push(w, vehicle, [-x, 0.0, -z]);
                [x / 4.0, 0.0, z / 4.0]
            })
        } else if is_boat(w, vehicle) && rules::boat_pushes(bb, player_box(&p), false) {
            push_impulse(vp, feet).map(|v| [v.x, v.y, v.z])
        } else {
            None
        };
        if let Some(impulse) = impulse {
            emit(w, VehiclePushEvent::PushPlayer { vehicle, player: Who::Player(id), impulse });
        }
    }
}

/// The step of a moved vehicle: each entity its box touches is pushed, then the players (the end of `AbstractBoat.tick`, the cart's push).
/// Only the vehicle's index cells are read; a vehicle at rest is not visited.
pub fn push_pairs(w: &mut World, vehicle: Entity) {
    w.init_resource::<PushBuf>();
    let mut buf = std::mem::take(&mut *w.resource_mut::<PushBuf>());
    contacts(w, vehicle, TOUCH_REACH, &mut buf.contacts);
    for &other in &buf.contacts {
        push_between(w, vehicle, other);
    }
    push_players(w, vehicle, &mut buf.players);
    *w.resource_mut::<PushBuf>() = buf;
}

/// The events written since the last call, in order.
pub fn drain_events(w: &mut World) -> Vec<VehiclePushEvent> {
    w.init_resource::<VehiclePushEvents>();
    std::mem::take(&mut w.resource_mut::<VehiclePushEvents>().0)
}

/// The pushes have no system of their own: `push_pairs` is called by the vehicle's movement.
pub fn register(_s: &mut bevy_ecs::schedule::Schedule) {}

#[cfg(test)]
mod tests {
    use super::*;

    fn world() -> World {
        let mut w = World::new();
        w.init_resource::<Clock>();
        w.init_resource::<Online>();
        w.init_resource::<crate::hostile::PlayerState>();
        w.insert_resource(Index::new(-32.0, 32));
        w
    }
    fn indexed(w: &mut World, e: Entity, at: Vec3) {
        let cell = w.resource::<Index>().cell_of(at.x, at.z);
        w.resource_mut::<Index>().insert(e, cell);
    }
    fn boat(w: &mut World, at: Vec3) -> Entity {
        let b = Aabb::new(Vec3::new(at.x - 0.5, at.y, at.z - 0.5), Vec3::new(at.x + 0.5, at.y + 1.0, at.z + 0.5));
        let e = w.spawn((BoatMarker, Pos(at), Vel(Vec3::new(0.0, 0.0, 0.0)), Bounds(b))).id();
        indexed(w, e, at);
        e
    }
    fn cart(w: &mut World, at: Vec3) -> Entity {
        let b = Aabb::new(Vec3::new(at.x - 0.49, at.y, at.z - 0.49), Vec3::new(at.x + 0.49, at.y + 0.7, at.z + 0.49));
        let e = w.spawn((Cart, Pos(at), Vel(Vec3::new(0.0, 0.0, 0.0)), Bounds(b))).id();
        indexed(w, e, at);
        e
    }
    fn creature(w: &mut World, at: Vec3) -> Entity {
        let b = Aabb::new(Vec3::new(at.x - 0.3, at.y, at.z - 0.3), Vec3::new(at.x + 0.3, at.y + 1.8, at.z + 0.3));
        let e = w.spawn((Pos(at), Vel(Vec3::new(0.0, 0.0, 0.0)), Bounds(b))).id();
        indexed(w, e, at);
        e
    }

    #[test]
    fn two_boats_push_each_other_by_equal_opposite_impulses() {
        let mut w = world();
        let (a, b) = (boat(&mut w, Vec3::new(0.0, 0.0, 0.0)), boat(&mut w, Vec3::new(0.6, 0.0, 0.0)));
        push_between(&mut w, a, b);
        // Offset 0.6 on x: the unit direction times 1 gives 0.05 times the root of the offset, the same for both.
        let expected = 0.05 * 0.6_f64.sqrt();
        assert!((w.get::<Vel>(b).unwrap().0.x - expected).abs() < 1e-8);
        assert!((w.get::<Vel>(a).unwrap().0.x + expected).abs() < 1e-8);
        assert!(drain_events(&mut w).is_empty(), "a bump between two boats says nothing to the host");
    }

    #[test]
    fn a_cart_pushes_a_creature_by_a_quarter_and_is_pushed_back() {
        let mut w = world();
        let (c, k) = (cart(&mut w, Vec3::new(0.0, 0.0, 0.0)), creature(&mut w, Vec3::new(0.9, 0.0, 0.0)));
        push_pairs(&mut w, c);
        let pushed = w.get::<PushIn>(k).expect("the creature takes a push");
        assert!((pushed.0.x - 0.0125).abs() < 1e-7 && pushed.0.z.abs() < 1e-12);
        assert!((w.get::<Vel>(c).unwrap().0.x + 0.05).abs() < 1e-7);
    }

    #[test]
    fn two_carts_facing_each_other_push_apart_by_the_creature_impulse() {
        let mut w = world();
        let (a, b) = (cart(&mut w, Vec3::new(0.0, 0.0, 0.0)), cart(&mut w, Vec3::new(0.9, 0.0, 0.0)));
        push_between(&mut w, a, b);
        assert!((w.get::<Vel>(a).unwrap().0.x + 0.05).abs() < 1e-7);
        assert!((w.get::<Vel>(b).unwrap().0.x - 0.05).abs() < 1e-7);
    }

    #[test]
    fn a_lone_vehicle_writes_no_event_and_pushes_nothing() {
        let mut w = world();
        let v = boat(&mut w, Vec3::new(0.0, 0.0, 0.0));
        push_pairs(&mut w, v);
        assert!(drain_events(&mut w).is_empty());
        assert!(w.get::<Moving>(v).is_none());
    }
}
