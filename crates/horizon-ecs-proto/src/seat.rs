//! Seats of a vehicle (a boat, a cart, a horse): who sits where. The vehicle holds `Seats`, its riders by seat (a player by its id, a
//! creature by its entity); a creature rider holds `Seated` (its vehicle and its seat). A player's vehicle is in `PlayerSeats` and in
//! `vehicle::set`, which wakes the creatures that follow it. Nothing runs each tick: `follow` places the riders when the vehicle moved.
use crate::cow::*;
use crate::goals::*;
use crate::host_out_seat::SeatEvent;
use crate::rules::seat_rules as rules;
use crate::vehicle::{self, Vehicle as View};
use bevy_ecs::prelude::*;
use horizon_core::geometry::Vec3;
use std::collections::HashMap;

pub use crate::rules::seat_rules::SeatKind;

/// Who sits in a seat: a player (an id of the host, not an entity) or a creature.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rider {
    Player(u32),
    Creature(Entity),
}
/// The seats of a vehicle, in seat order (`None` is a free seat).
#[derive(Component, Clone, Debug)]
pub struct Seats {
    pub kind: SeatKind,
    pub riders: Vec<Option<Rider>>,
}
impl Seats {
    pub fn new(kind: SeatKind) -> Self {
        Self { kind, riders: vec![None; rules::capacity(kind)] }
    }
}
/// A creature that sits in a vehicle: the vehicle and the seat.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Seated {
    pub vehicle: Entity,
    pub slot: u8,
}
/// The vehicle of each seated player: read by its id only.
#[derive(Resource, Default)]
pub struct PlayerSeats(pub HashMap<u32, Entity>);
/// The vehicles that took a rider (a seat or a mount), marked at the boarding and pruned when read (`ridden_vehicles`): no walk of the
/// mounts. A vehicle stays in it until a read finds it empty.
#[derive(Resource, Default)]
pub struct RiddenVehicles(pub std::collections::HashSet<Entity>);
/// Marks `vehicle` as carrying a rider.
pub fn mark_ridden(w: &mut World, vehicle: Entity) {
    w.init_resource::<RiddenVehicles>();
    w.resource_mut::<RiddenVehicles>().0.insert(vehicle);
}
/// The seat events of the step, drained by the bridge.
#[derive(Resource, Default)]
pub struct SeatEvents(pub Vec<SeatEvent>);

fn push_event(w: &mut World, e: SeatEvent) {
    w.init_resource::<SeatEvents>();
    w.resource_mut::<SeatEvents>().0.push(e);
}
/// What the vehicle is for `vehicle::set`, read from its components.
fn view_of(w: &World, vehicle: Entity) -> Option<View> {
    let (pos, kind) = (w.get::<Pos>(vehicle)?.0, w.get::<Seats>(vehicle)?.kind);
    let yaw = w.get::<Yaw>(vehicle).map_or(0.0, |y| y.0);
    let velocity = w.get::<Vel>(vehicle).map_or(Vec3::new(0.0, 0.0, 0.0), |v| v.0);
    Some(View { nautilus: false, bounds: rules::vehicle_bounds(kind, pos), velocity, yaw, passenger: true })
}
/// Takes seat `slot` (or the first free one) of `vehicle` for `who`; None when there is no such free seat.
fn take(w: &mut World, vehicle: Entity, who: Rider, slot: Option<u8>) -> Option<u8> {
    if !ensure_seats(w, vehicle) {
        return None;
    }
    let mut s = w.get_mut::<Seats>(vehicle)?;
    let i = match slot {
        Some(i) => i as usize,
        None => s.riders.iter().position(Option::is_none)?,
    };
    match s.riders.get_mut(i) {
        Some(seat) if seat.is_none() => {
            *seat = Some(who);
            mark_ridden(w, vehicle);
            Some(i as u8)
        }
        _ => None,
    }
}
/// Frees the seat of `who` in `vehicle`.
fn clear(w: &mut World, vehicle: Entity, who: Rider) {
    if let Some(mut s) = w.get_mut::<Seats>(vehicle)
        && let Some(seat) = s.riders.iter_mut().find(|r| **r == Some(who))
    {
        *seat = None;
    }
    if w.get::<Seats>(vehicle).is_some_and(|s| s.riders.iter().all(Option::is_none)) {
        w.entity_mut(vehicle).remove::<Seats>();
    }
}
/// The seats a vehicle takes, read from its components: a boat (`boat::Kind`), a cart (`minecart::Kind`); None for any other entity.
pub fn seats_of(w: &World, vehicle: Entity) -> Option<Seats> {
    if let Some(k) = w.get::<crate::boat::Kind>(vehicle) {
        return Some(Seats::new(SeatKind::Boat { chest: k.chest, raft: k.raft }));
    }
    w.get::<crate::minecart::Kind>(vehicle).map(|_| Seats::new(SeatKind::Cart))
}
/// Puts `Seats` on `vehicle` when it has none (the first boarding); false when it is not a vehicle that seats.
fn ensure_seats(w: &mut World, vehicle: Entity) -> bool {
    if w.get::<Seats>(vehicle).is_some() {
        return true;
    }
    let Some(s) = seats_of(w, vehicle) else { return false };
    w.entity_mut(vehicle).insert(s);
    true
}
/// The riders sit where the vehicle is: each seat is put at its offset. A creature is moved, turned and woken; a player's place is written
/// and `vehicle::set` wakes its followers, only when the place changed.
fn place_riders(w: &mut World, vehicle: Entity) {
    let (Some(pos), Some(s)) = (w.get::<Pos>(vehicle).map(|p| p.0), w.get::<Seats>(vehicle).cloned()) else { return };
    let yaw = w.get::<Yaw>(vehicle).map_or(0.0, |y| y.0);
    let count = s.riders.iter().flatten().count();
    for (rank, r) in s.riders.iter().flatten().enumerate() {
        let at = rules::seat_point(pos, yaw, rules::seat_offset(s.kind, rank, count), rules::seat_height(s.kind));
        match *r {
            Rider::Creature(e) => {
                let moved = w.get::<Pos>(e).is_some_and(|p| p.0 != at);
                if moved {
                    if let Some(mut p) = w.get_mut::<Pos>(e) {
                        p.0 = at;
                    }
                    w.entity_mut(e).insert(Moving);
                }
                if let Some(mut y) = w.get_mut::<Yaw>(e) {
                    y.0 = yaw;
                }
            }
            Rider::Player(p) => {
                if let Some(q) = w.resource::<Online>().get(p)
                    && (q.x, q.y, q.z) != (at.x, at.y, at.z)
                {
                    w.resource_mut::<Online>().set(Player { x: at.x, y: at.y, z: at.z, ..q });
                    if let Some(view) = view_of(w, vehicle) {
                        vehicle::set(w, p, Some(view));
                    }
                }
            }
        }
    }
}
/// The vehicle moved: its riders are placed again (the entry point of the vehicle's movement system, no system of its own).
pub fn follow(w: &mut World, vehicle: Entity) {
    place_riders(w, vehicle);
}
/// `Entity.startRiding` of a vehicle: the creature `rider` takes seat `slot` (or the first free one); false when it already sits or the
/// seats are taken.
pub fn board(w: &mut World, rider: Entity, vehicle: Entity, slot: Option<u8>) -> bool {
    if w.get::<Seated>(rider).is_some() {
        return false;
    }
    let Some(slot) = take(w, vehicle, Rider::Creature(rider), slot) else { return false };
    w.entity_mut(rider).insert(Seated { vehicle, slot });
    place_riders(w, vehicle);
    push_event(w, SeatEvent::Boarded { rider: Rider::Creature(rider), vehicle });
    true
}
/// `Entity.stopRiding` of a creature: its seat is freed and the other riders are placed again. Returns the vehicle's point, from which
/// the host finds where to get off (`boat_dismount_target`), None when the creature sits nowhere.
pub fn leave(w: &mut World, rider: Entity) -> Option<[f64; 3]> {
    let Seated { vehicle, .. } = w.get::<Seated>(rider).copied()?;
    w.entity_mut(rider).remove::<Seated>();
    clear(w, vehicle, Rider::Creature(rider));
    place_riders(w, vehicle);
    push_event(w, SeatEvent::Left { rider: Rider::Creature(rider), vehicle });
    let p = w.get::<Pos>(vehicle)?.0;
    Some([p.x, p.y, p.z])
}
/// `board` for a player: the player takes a seat of `vehicle` unless it already rides one.
pub fn seat_player(w: &mut World, player: u32, vehicle: Entity, slot: Option<u8>) -> bool {
    w.init_resource::<PlayerSeats>();
    if w.resource::<PlayerSeats>().0.contains_key(&player) || take(w, vehicle, Rider::Player(player), slot).is_none() {
        return false;
    }
    w.resource_mut::<PlayerSeats>().0.insert(player, vehicle);
    if let Some(view) = view_of(w, vehicle) {
        vehicle::set(w, player, Some(view));
    }
    place_riders(w, vehicle);
    push_event(w, SeatEvent::Boarded { rider: Rider::Player(player), vehicle });
    true
}
/// The player gets off its vehicle: its seat is freed, the vehicle's other riders are placed again, and `vehicle::set` wakes its followers.
pub fn unseat_player(w: &mut World, player: u32) {
    w.init_resource::<PlayerSeats>();
    let Some(vehicle) = w.resource_mut::<PlayerSeats>().0.remove(&player) else { return };
    clear(w, vehicle, Rider::Player(player));
    vehicle::set(w, player, None);
    place_riders(w, vehicle);
    push_event(w, SeatEvent::Left { rider: Rider::Player(player), vehicle });
}
/// `Entity.ejectPassengers` of a vehicle: every player is unseated (its id is read by `players_of` before), every creature gets off. The
/// creatures come back in seat order.
pub fn eject_all(w: &mut World, vehicle: Entity) -> Vec<Entity> {
    let Some(mut s) = w.get_mut::<Seats>(vehicle) else { return Vec::new() };
    let riders: Vec<Rider> = s.riders.iter_mut().filter_map(Option::take).collect();
    drop(s);
    w.entity_mut(vehicle).remove::<Seats>();
    let mut creatures = Vec::new();
    for r in riders {
        push_event(w, SeatEvent::Left { rider: r, vehicle });
        match r {
            Rider::Creature(e) => {
                w.entity_mut(e).remove::<Seated>();
                creatures.push(e);
            }
            Rider::Player(p) => {
                w.init_resource::<PlayerSeats>();
                w.resource_mut::<PlayerSeats>().0.remove(&p);
                vehicle::set(w, p, None);
            }
        }
    }
    creatures
}
/// The creatures that sit in `vehicle`, in seat order: read from the vehicle's seats, not from the creatures.
pub fn riders_of(w: &World, vehicle: Entity) -> Vec<Entity> {
    let Some(s) = w.get::<Seats>(vehicle) else { return Vec::new() };
    s.riders.iter().flatten().filter_map(|r| match r {
        Rider::Creature(e) => Some(*e),
        Rider::Player(_) => None,
    }).collect()
}
/// The players that sit in `vehicle`, in seat order.
pub fn players_of(w: &World, vehicle: Entity) -> Vec<u32> {
    let Some(s) = w.get::<Seats>(vehicle) else { return Vec::new() };
    s.riders.iter().flatten().filter_map(|r| match r {
        Rider::Player(p) => Some(*p),
        Rider::Creature(_) => None,
    }).collect()
}
pub fn vehicle_of(w: &World, rider: Entity) -> Option<Entity> {
    w.get::<Seated>(rider).map(|s| s.vehicle)
}
pub fn free_seat(w: &World, vehicle: Entity) -> bool {
    let free = |s: &Seats| s.riders.iter().any(Option::is_none);
    w.get::<Seats>(vehicle).map_or_else(|| seats_of(w, vehicle).is_some_and(|s| free(&s)), free)
}
/// The seats have no system of their own: `follow` is called by the vehicle's movement.
pub fn register(_s: &mut Schedule) {}

#[cfg(test)]
mod tests {
    use super::*;

    fn world() -> World {
        let mut w = World::new();
        w.init_resource::<Events>();
        w.init_resource::<Online>();
        w
    }
    fn boat(w: &mut World, chest: bool, raft: bool, at: Vec3) -> Entity {
        w.spawn((crate::boat::Kind { raft, chest }, Seats::new(SeatKind::Boat { chest, raft }), Pos(at), Yaw(0.0), Vel(Vec3::new(0.0, 0.0, 0.0)))).id()
    }
    fn creature(w: &mut World) -> Entity {
        w.spawn(Pos(Vec3::new(0.0, 0.0, 0.0))).id()
    }

    #[test]
    fn capacity_follows_the_boat_rules() {
        let mut w = world();
        let chest = boat(&mut w, true, false, Vec3::new(0.0, 0.0, 0.0));
        let (a, b) = (creature(&mut w), creature(&mut w));
        assert!(board(&mut w, a, chest, None));
        assert!(!board(&mut w, b, chest, None));
        let plain = boat(&mut w, false, false, Vec3::new(0.0, 0.0, 0.0));
        assert!(board(&mut w, b, plain, None));
        let c = creature(&mut w);
        assert!(board(&mut w, c, plain, None));
        let d = creature(&mut w);
        assert!(!board(&mut w, d, plain, None));
        let raft = boat(&mut w, true, true, Vec3::new(0.0, 0.0, 0.0));
        assert_eq!(w.get::<Seats>(raft).unwrap().riders.len(), 1);
    }

    #[test]
    fn second_rider_is_offset_by_seat_z_and_follows_the_boat() {
        let mut w = world();
        let plain = boat(&mut w, false, false, Vec3::new(0.0, 0.0, 0.0));
        let (a, b) = (creature(&mut w), creature(&mut w));
        assert!(board(&mut w, a, plain, None));
        assert!(board(&mut w, b, plain, None));
        // Yaw 0 looks along +z: the first seat is 0,2 ahead, the second 0,6 behind (`seat_z`).
        let za = w.get::<Pos>(a).unwrap().0.z;
        let zb = w.get::<Pos>(b).unwrap().0.z;
        assert!((za - 0.2).abs() < 1e-6 && (zb + 0.6).abs() < 1e-6);
        w.get_mut::<Pos>(plain).unwrap().0 = Vec3::new(5.0, 0.0, 5.0);
        follow(&mut w, plain);
        assert!((w.get::<Pos>(b).unwrap().0.z - 4.4).abs() < 1e-6);
    }

    #[test]
    fn eject_all_returns_the_creatures_in_boarding_order() {
        let mut w = world();
        let v = boat(&mut w, false, false, Vec3::new(0.0, 0.0, 0.0));
        let (a, b) = (creature(&mut w), creature(&mut w));
        assert!(board(&mut w, a, v, None));
        assert!(board(&mut w, b, v, None));
        assert_eq!(riders_of(&w, v), vec![a, b]);
        assert_eq!(eject_all(&mut w, v), vec![a, b]);
        assert!(riders_of(&w, v).is_empty() && vehicle_of(&w, a).is_none() && free_seat(&w, v));
    }

    #[test]
    fn a_player_leaving_wakes_its_followers() {
        let mut w = world();
        let v = boat(&mut w, false, false, Vec3::new(0.0, 0.0, 0.0));
        assert!(seat_player(&mut w, 7, v, None));
        for _ in 0..2 {
            w.resource_mut::<Events>().vehicle_moved.swap();
        }
        unseat_player(&mut w, 7);
        w.resource_mut::<Events>().vehicle_moved.swap();
        assert!(w.resource::<Events>().vehicle_moved.iter().any(|v| v == 7));
        assert!(free_seat(&w, v));
    }
}
