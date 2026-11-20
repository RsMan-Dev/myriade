//! The boat's physics on sampled facts, against the vanilla constants of `AbstractBoat` (read in the jar: the drags 0.9 and 0.45, the
//! buoyancy 0.01 and 0.04/0.65, the flowing sink -7e-4, the paddle 0.04/0.005, the seats 0.2 and -0.6, the chest 0.15) and values
//! worked by hand. Horizon's own boat tick lives in the game crate, not reachable from here: no tick-by-tick oracle yet.
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::boat::*;

fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-6, "{a} vs {b}");
}

#[test]
fn a_flowing_fluid_over_the_hull_wins_the_status() {
    assert_eq!(classify(Under::Flowing, true, 0.5), Status::UnderFlowingWater);
    assert_eq!(classify(Under::Still, true, 0.5), Status::UnderWater);
    assert_eq!(classify(Under::Dry, true, 0.5), Status::InWater);
    assert_eq!(classify(Under::Dry, false, 0.5), Status::OnLand);
    assert_eq!(classify(Under::Dry, false, 0.0), Status::InAir);
}

#[test]
fn a_boat_reaching_the_water_from_the_air_is_lifted_once() {
    let mut b = Boat::default();
    assert!(!set_status(&mut b, Status::InAir, 0.0));
    assert!(set_status(&mut b, Status::InWater, 0.0), "the first water after the air");
    assert!(!set_status(&mut b, Status::InWater, 0.0), "already in the water");
}

#[test]
fn in_water_the_buoyancy_lifts_a_still_boat_by_the_jar_numbers() {
    let mut b = Boat { water_level: HEIGHT, ..Boat::default() };
    set_status(&mut b, Status::InWater, 0.0);
    let mut vel = Vec3::new(0.0, 0.0, 0.0);
    let mut yaw = 0.0_f32;
    // Buoyancy (water_level - y) / HEIGHT = 1, gravity 0.04: (-0.04 + 0.04 / 0.65) * 0.75.
    float(&mut b, &mut vel, &mut yaw, 0.0, 0.04, false);
    near(vel.y, (-0.04 + 0.04 / 0.65) * 0.75);
    near(vel.x, 0.0);
}

#[test]
fn under_a_still_fluid_the_drag_is_0_45_and_the_sink_is_the_gravity() {
    let mut b = Boat::default();
    set_status(&mut b, Status::UnderWater, 0.0);
    let mut vel = Vec3::new(1.0, 0.0, 1.0);
    let mut yaw = 0.0_f32;
    float(&mut b, &mut vel, &mut yaw, 0.0, 0.04, false);
    near(vel.x, 0.45);
    near(vel.z, 0.45);
    near(vel.y, (-0.04 + 0.01 * 0.04 / 0.65) * 0.75);
}

#[test]
fn a_rider_pressing_up_pushes_along_the_yaw_at_0_04() {
    let mut b = Boat { input: [false, false, true, false], ..Boat::default() };
    set_status(&mut b, Status::InAir, 0.0);
    let mut vel = Vec3::new(0.0, 0.0, 0.0);
    let mut yaw = 0.0_f32;
    float(&mut b, &mut vel, &mut yaw, 0.0, 0.0, true);
    near(vel.x, 0.0);
    near(vel.z, 0.04);
    assert_eq!(b.paddling, [true, true]);
}

#[test]
fn a_rider_pressing_left_turns_the_boat_and_paddles_the_second_row() {
    let mut b = Boat { input: [true, false, false, false], ..Boat::default() };
    set_status(&mut b, Status::InAir, 0.0);
    let mut vel = Vec3::new(0.0, 0.0, 0.0);
    let mut yaw = 0.0_f32;
    float(&mut b, &mut vel, &mut yaw, 0.0, 0.0, true);
    near(f64::from(yaw), -1.0);
    assert_eq!(b.paddling, [false, true]);
}

#[test]
fn a_chest_boat_seats_one_and_a_boat_two() {
    let boat = Kind { raft: false, chest: false };
    let chest = Kind { raft: false, chest: true };
    assert_eq!(max_passengers(boat), 2);
    assert_eq!(max_passengers(chest), 1);
    near(f64::from(seat_z(chest, 0, 1, false)), 0.15);
    near(f64::from(seat_z(boat, 0, 2, false)), 0.2);
    near(f64::from(seat_z(boat, 1, 2, false)), -0.6);
    near(f64::from(seat_z(boat, 0, 2, true)), 0.4);
}

#[test]
fn a_boat_sits_a_third_of_its_height_up_and_a_raft_eight_ninths() {
    near(ride_height(Kind { raft: false, chest: false }), HEIGHT / 3.0);
    near(ride_height(Kind { raft: true, chest: false }), HEIGHT * 8.0 / 9.0);
}
