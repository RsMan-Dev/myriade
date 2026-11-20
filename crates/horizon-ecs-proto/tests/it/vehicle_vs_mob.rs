//! The dolphin that follows the vehicle of a player (`HORIZON_SPECIES=minecraft:dolphin`, `HORIZON_TEST_RESOURCES` set), against the `Mob` dolphin with the
//! vehicle imposed: when the goal starts and ends (the vehicle starts moving near it, stops, is a nautilus not a boat, is too far, the player gets off).
//! The dry slab of the bench has no water: the swim itself is not compared (the `Mob`'s dolphin also strands); the DECISIONS are.
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_client_lib::mobs::Vehicle as MobVehicle;
use horizon_core::geometry::{Aabb, Vec3};
use horizon_ecs_proto::behaviours::follow_player_vehicle::Escorting;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::goals::*;
use horizon_ecs_proto::vehicle::{self, Vehicle};

fn skip() -> bool {
    !have_data() || std::env::var("HORIZON_SPECIES").as_deref() != Ok("minecraft:dolphin")
}
fn boxed(at: Vec3) -> Aabb {
    Aabb::new(Vec3::new(at.x - 0.7, at.y, at.z - 0.7), Vec3::new(at.x + 0.7, at.y + 0.6, at.z + 0.7))
}
/// The goal runs on each side: (ticks it ran at, as (first, last)).
fn scenario(at: Vec3, nautilus_vehicle: bool, moving: impl Fn(u32) -> bool, passenger: impl Fn(u32) -> bool) -> ((Option<u32>, Option<u32>), (Option<u32>, Option<u32>)) {
    let mut r = MobRef::new(60, false);
    let mut h = Herd::with_slab(60, true, false);
    let i = r.add_id(Vec3::new(0.5, 1.0, 0.5), 31_337);
    let e = spawn_species(&mut h.world, species(), 0.5, 0.5, 5);
    h.world.resource_mut::<Entered>().0.push(e);
    let (mut m, mut x) = ((None, None), (None, None));
    for t in 1..=140u32 {
        let v = Vec3::new(0.0, 0.0, if moving(t) { 0.3 } else { 0.0 });
        let p = Vec3::new(at.x, at.y, at.z + if t > 10 { 0.3 * f64::from((t - 10).min(60)) } else { 0.0 });
        let mut view = player_view(p, None);
        view.passenger = passenger(t);
        view.vehicle = Some(MobVehicle { nautilus: nautilus_vehicle, bounds: boxed(p), velocity: v });
        h.world.resource_mut::<Online>().set(Player { id: 1, x: p.x, y: p.y, z: p.z, main: 0, off: 0, ..Default::default() });
        vehicle::set(&mut h.world, 1, Some(Vehicle { nautilus: nautilus_vehicle, bounds: boxed(p), velocity: v, yaw: 0.0, passenger: passenger(t) }));
        r.step(Some(&view));
        h.tick();
        if r.names(i).iter().any(|n| *n == "FollowPlayerRiddenEntityGoal") {
            m = (m.0.or(Some(t)), Some(t));
        }
        if h.world.get::<Escorting>(e).is_some() {
            x = (x.0.or(Some(t)), Some(t));
        }
    }
    (m, x)
}

/// A boat that starts moving at tick 10 within 5 blocks, stops at tick 70: the goal starts and ends within a tick or two on both sides.
#[test]
fn the_dolphin_follows_a_moving_boat_and_leaves_when_it_stops() {
    if skip() {
        return;
    }
    let (m, x) = scenario(Vec3::new(4.5, 1.0, 0.5), false, |t| (11..=70).contains(&t), |_| true);
    eprintln!("boat near, moving 11..70: goal ran at ticks mob {m:?} ecs {x:?}");
    let (a, b) = (m.0.expect("the Mob starts"), x.0.expect("the ecs starts"));
    assert!(b.abs_diff(a) <= 2, "start: mob {a} ecs {b}");
    let (a, b) = (m.1.unwrap(), x.1.unwrap());
    assert!(a < 100 && b.abs_diff(a) <= 3, "end: mob {a} ecs {b}");
}
/// A boat too far (12 blocks), a nautilus for a boat's goal, a player that is not a passenger: the goal does not run on either side.
#[test]
fn the_dolphin_ignores_a_far_vehicle_the_wrong_kind_and_a_rider_that_got_off() {
    if skip() {
        return;
    }
    let far = scenario(Vec3::new(16.5, 1.0, 0.5), false, |t| t > 10, |_| true);
    // A nautilus near: only the second instance (the nautilus's) runs; the goal runs on both sides.
    let nautilus = scenario(Vec3::new(4.5, 1.0, 0.5), true, |t| (11..=70).contains(&t), |_| true);
    // The player gets off at tick 40: the goal ends then.
    let off = scenario(Vec3::new(4.5, 1.0, 0.5), false, |t| t > 10, |t| t < 40);
    eprintln!("far: {far:?}; nautilus near: {nautilus:?}; gets off at 40: {off:?}");
    assert!(far.0.0.is_none() && far.1.0.is_none(), "{far:?}");
    assert!(nautilus.0.0.is_some() && nautilus.1.0.is_some(), "{nautilus:?}");
    assert!(off.0.1.is_some_and(|t| t <= 42) && off.1.1.is_some_and(|t| t <= 42), "{off:?}");
}
