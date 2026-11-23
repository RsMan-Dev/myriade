//! Lot 7 of the rules of the living mobs (`living.rs` tests): the block factors of the ground a living mob stands on, the fall a mob
//! slides down on vines, the lift of a bubble column, and the fluids at a mob's eyes. Expected values are vanilla's (`Blocks`: honey
//! `speedFactor` 0.4 and `jumpFactor` 0.5, stone the defaults 1.0; `LivingEntity.handleOnClimbable`: a fall at most 0.15; `Entity`:
//! a bubble column adds 0.06 to the rise per block it touches, capped at 0.7).
use crate::common::*;
use myriade_engine_lib::mobs::MobWorld;
use horizon_core::geometry::Vec3;
use horizon_core::position::BlockPos;
use horizon_ecs_proto::bridge::host::HostSetup;
use horizon_ecs_proto::cow::{Drive, FluidState, Herd, Level, Vel};
use horizon_ecs_proto::rules::climbable;
use horizon_ecs_proto::targeting::Who;
use horizon_ecs_proto::{block_factor, effects::{self, Kind}, level, mount::{self, Mount}, world_push};

fn mw(h: &Herd) -> &'static MobWorld<'static> {
    h.world.resource::<Level>().mw.0
}

fn vy(h: &Herd, e: bevy_ecs::entity::Entity) -> f64 {
    h.world.get::<Vel>(e).unwrap().0.y
}

#[test]
fn honey_slows_the_point_above_it_like_soul_sand() {
    let mut h = Herd::with_slab(4, false, false);
    h.set_block(1, 0, 0, "minecraft:honey_block");
    h.tick();
    assert!((block_factor::speed(mw(&h), [1.5, 1.0, 0.5]) - 0.4).abs() < 1e-6);
}

#[test]
fn stone_leaves_the_jump_at_one() {
    let mut h = Herd::with_slab(4, false, false);
    h.set_block(3, 0, 0, "minecraft:stone");
    h.tick();
    assert_eq!(block_factor::jump(mw(&h), [3.5, 1.0, 0.5]), 1.0);
}

#[test]
fn vines_cap_the_fall_and_the_sideways_move_at_a_point_fifteen() {
    let v = climbable::velocity(Vec3::new(0.4, -0.5, -0.4));
    assert!((v.y + 0.15).abs() < 1e-6, "the fall is at most 0.15 a tick, got {}", v.y);
    assert!((v.x - 0.15).abs() < 1e-6 && (v.z + 0.15).abs() < 1e-6, "the move across is at most 0.15 each way");
    let rest = climbable::velocity(Vec3::new(0.0, -0.05, 0.0));
    assert_eq!(rest.y, -0.05, "a slower fall is kept");
}

#[test]
fn vines_hold_a_falling_mob_to_a_point_fifteen_from_the_third_tick() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    for y in 1..=8 {
        h.set_block_state(0, y, 0, "minecraft:vine", &[("south", "true")]);
        h.set_block(0, y, 1, "minecraft:stone");
    }
    let open = ecs_cow(&mut h, 5.5, 6.0, 5.5, 8);
    let vine = ecs_cow(&mut h, 0.5, 6.0, 0.5, 9);
    // Each tick: the climb clamps the fall to 0.15 first (`handleOnClimbable`), then gravity 0.08 and drag 0.98 are applied.
    // From rest: -0.0784, -0.155232 (the clamp is not reached yet), then the vine gives -0.23 * 0.98 = -0.2254 where the open air gives -0.230527.
    let mut open_v = Vec::new();
    let mut vine_v = Vec::new();
    for _ in 0..3 {
        h.tick();
        open_v.push(vy(&h, open));
        vine_v.push(vy(&h, vine));
    }
    assert!((open_v[0] + 0.0784).abs() < 1e-4, "open air, first tick: {}", open_v[0]);
    assert!((open_v[2] + 0.230527).abs() < 1e-4, "open air accelerates again, third tick: {}", open_v[2]);
    assert!((vine_v[1] + 0.155232).abs() < 1e-4, "not yet capped on the second tick: {}", vine_v[1]);
    assert!((vine_v[2] + 0.2254).abs() < 1e-4, "capped at 0.15 before gravity from the third tick: {}", vine_v[2]);
}

#[test]
fn a_bubble_column_lifts_a_mob_once_for_each_block_it_touches() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    // Upward columns (`drag=false`) over water: one bubble block under the first cow, two under the second.
    h.set_block(0, 1, 0, "minecraft:water");
    h.set_block(5, 1, 0, "minecraft:water");
    h.set_block_state(0, 2, 0, "minecraft:bubble_column", &[("drag", "false")]);
    h.set_block_state(5, 2, 0, "minecraft:bubble_column", &[("drag", "false")]);
    h.set_block_state(5, 3, 0, "minecraft:bubble_column", &[("drag", "false")]);
    let one = ecs_cow(&mut h, 0.5, 2.0, 0.5, 6);
    // At 2.5 the box (0.9 tall) also meets the block at 3: two blocks touched, two lifts.
    let two = ecs_cow(&mut h, 5.5, 2.5, 0.5, 7);
    h.tick();
    let (a, b) = (vy(&h, one), vy(&h, two));
    assert!((b - a - 0.06).abs() < 1e-3, "the second block adds 0.06 to the rise: {a} then {b}");
}

#[test]
fn a_mob_in_lava_tracks_lava_at_its_eyes() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    for y in 1..=4 {
        h.set_block(0, y, 0, "minecraft:lava");
    }
    let e = ecs_cow(&mut h, 0.5, 1.0, 0.5, 6);
    h.tick();
    let fs = *h.world.get::<FluidState>(e).unwrap();
    assert!(fs.in_lava && fs.eye_in_lava, "feet and eyes in lava: {fs:?}");
    assert!(!fs.in_water && !fs.eye_in_water, "no water here: {fs:?}");
}

#[test]
fn a_jump_from_honey_is_half_the_power_and_one_from_stone_is_the_full_power() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    h.set_block(0, 0, 0, "minecraft:honey_block");
    h.set_block(4, 0, 0, "minecraft:stone");
    let honey = ecs_cow(&mut h, 0.5, 1.0, 0.5, 6);
    let stone = ecs_cow(&mut h, 4.5, 1.0, 0.5, 7);
    level::jump(&mut h.world, honey);
    level::jump(&mut h.world, stone);
    assert!((vy(&h, honey) - 0.21).abs() < 1e-6, "0.42 times the honey's jump factor 0.5: {}", vy(&h, honey));
    assert!((vy(&h, stone) - 0.42).abs() < 1e-6, "0.42 on stone: {}", vy(&h, stone));
}

#[test]
fn a_slime_block_bounces_at_one_and_a_bed_or_a_shelf_mushroom_at_three_quarters_as_its_data_says() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    h.set_block(0, 0, 0, "minecraft:slime_block");
    h.set_block(1, 0, 0, "minecraft:red_bed");
    h.set_block(2, 0, 0, "minecraft:shelf_mushroom");
    // `Blocks.<clinit>`: SLIME_BLOCK bounceRestitution 1.0; the wool beds and SHELF_MUSHROOM 0.75.
    assert_eq!(level::bounce_restitution(&h.world, BlockPos::new(0, 0, 0)), 1.0);
    assert_eq!(level::bounce_restitution(&h.world, BlockPos::new(1, 0, 0)), 0.75);
    assert_eq!(level::bounce_restitution(&h.world, BlockPos::new(2, 0, 0)), 0.75);
}

#[test]
fn a_mob_over_air_at_a_slime_edge_takes_the_slime_it_meets_as_its_support() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    h.set_block(0, 0, 0, "minecraft:slime_block");
    h.set_block(1, 0, 0, "minecraft:air");
    // The box (0.9 wide, centre x 1.1) meets the slime at x 0..1 and the air at x 1..2; the centre is over the air.
    let e = ecs_cow(&mut h, 1.1, 1.0, 0.5, 6);
    let support = world_push::support_below(&h.world, e, Vec3::new(1.1, 1.0, 0.5), Vec3::ZERO);
    assert_eq!(support, BlockPos::new(0, 0, 0), "the nearest block the box meets, as `getOnPos` gives");
    assert_eq!(level::bounce_restitution(&h.world, support), 1.0);
}

#[test]
fn jump_boost_adds_a_tenth_per_level_to_the_jump_power() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    h.set_block(0, 0, 0, "minecraft:stone");
    let e = ecs_cow(&mut h, 0.5, 1.0, 0.5, 6);
    // `getJumpBoostPower`: 0.1 x (amplifier + 1); level 2 gives 0.2 on top of 0.42.
    effects::add(&mut h.world, e, Kind::JumpBoost, 1, 100);
    level::jump(&mut h.world, e);
    assert!((vy(&h, e) - 0.62).abs() < 1e-6, "0.42 + 0.2: {}", vy(&h, e));
}

#[test]
fn a_steered_saddled_horse_has_a_tenth_of_its_speed_as_its_air_speed() {
    if !have_data() {
        return;
    }
    let setup = HostSetup { seed: 7, dimension: "overworld", simulation_radius: 4 };
    let (mw, mut h) = awake_host(&setup);
    let horse = h.spawn(mw, "minecraft:horse", [0.5, 64.0, 0.5], None).expect("a known species");
    h.world_mut().get_mut::<Mount>(horse).unwrap().saddled = true;
    assert!(mount::ride(h.world_mut(), horse, Who::Player(1)), "the first seat is free");
    h.ride_input(horse, [0.0, 1.0], 0.0, false, false, false, 0.0, 0.0);
    h.step(mw);
    // `LivingEntity.getFlyingSpeed` of a controlled mount: 0.1 times its speed (0.02 when it has no rider).
    let d = h.world().get::<Drive>(horse).unwrap();
    assert!(d.air_speed.is_some_and(|a| (a - d.speed * 0.1).abs() < 1e-6), "air speed {:?} for speed {}", d.air_speed, d.speed);
}

#[test]
fn a_boat_passenger_under_the_boat_top_measures_no_fluid_at_its_feet() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    h.set_block(0, 1, 0, "minecraft:lava");
    let e = ecs_cow(&mut h, 0.5, 1.0, 0.5, 6);
    // `AbstractBoat.modifyPassengerFluidInteractionBox`: the boat is in the air and its top is above the cow's: no box, no lava.
    let boat = horizon_ecs_proto::boat_state::spawn(&mut h.world, 0, false, [5.5, 2.0, 0.5], 0.0);
    let p = Vec3::new(5.5, 2.0, 0.5);
    h.world.entity_mut(boat).insert(horizon_ecs_proto::cow::Bounds(horizon_ecs_proto::rules::boat::bounds_at(p)));
    h.world.entity_mut(e).insert(horizon_ecs_proto::mount::Riding(boat));
    h.tick();
    let fs = *h.world.get::<FluidState>(e).unwrap();
    assert!(!fs.in_lava && !fs.eye_in_lava, "a passenger below its boat's top measures no lava: {fs:?}");
}

#[test]
fn a_boat_passenger_of_a_boat_under_water_keeps_the_box_and_measures_the_lava_at_its_feet() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    h.set_block(0, 1, 0, "minecraft:lava");
    let e = ecs_cow(&mut h, 0.5, 1.0, 0.5, 6);
    // `AbstractBoat.modifyPassengerFluidInteractionBox`: a boat under water leaves the passenger's box as it is.
    let p = Vec3::new(5.5, 2.0, 0.5);
    let boat = horizon_ecs_proto::boat_state::spawn(&mut h.world, 0, false, [5.5, 2.0, 0.5], 0.0);
    h.world.entity_mut(boat).insert(horizon_ecs_proto::cow::Bounds(horizon_ecs_proto::rules::boat::bounds_at(p)));
    h.world.get_mut::<horizon_ecs_proto::boat::Boat>(boat).unwrap().status = horizon_ecs_proto::rules::boat::Status::UnderWater;
    h.world.entity_mut(e).insert(horizon_ecs_proto::mount::Riding(boat));
    h.tick();
    let fs = *h.world.get::<FluidState>(e).unwrap();
    assert!(fs.in_lava, "a passenger of a boat under water keeps its own box: {fs:?}");
}
