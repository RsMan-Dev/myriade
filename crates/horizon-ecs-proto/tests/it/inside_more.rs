//! Cactus, lava, fire and honey on carts, boats, projectiles and creatures (`inside_more.rs`). Expected values are the jar's: a cactus hurts a
//! vehicle 1 (damage 10 on the hurt counter), lava lights a boat for 15 s, a fire source primes a TNT cart, a burning arrow goes out in water,
//! and a creature sliding on honey falls at `(-0.05 - 0.08) * 0.98` with its sideways speed scaled by `-0.05 / (vy / 0.98 + 0.08)`.
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::cart_state::CartHurt;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::inside::Sensitive;
use horizon_ecs_proto::inside_effects::{fire_left, ignite_projectile};
use horizon_ecs_proto::inside_more::{slide, sliding_down};
use horizon_ecs_proto::minecart::{Fuse, Kind};
use horizon_ecs_proto::projectile::{launch, Flight};
use std::sync::Arc;

fn world() -> Herd {
    let mut h = Herd::with_slab_awake(8, true, false);
    h.world.resource_mut::<Sensitive>().0 = Some(Arc::from(vec![0u64]));
    h
}

#[test]
fn a_cactus_hurts_a_cart() {
    if !have_data() {
        return;
    }
    let mut h = world();
    h.set_block(0, 1, 0, "minecraft:cactus");
    let c = horizon_ecs_proto::cart_state::spawn(&mut h.world, Kind::Rideable, [0.5, 1.0, 0.5], 0.0);
    h.tick();
    assert_eq!(h.world.get::<CartHurt>(c).map(|x| x.damage), Some(10.0));
}

#[test]
fn fire_primes_a_tnt_cart() {
    if !have_data() {
        return;
    }
    let mut h = world();
    h.set_block(0, 1, 0, "minecraft:fire");
    let c = horizon_ecs_proto::cart_state::spawn(&mut h.world, Kind::Tnt, [0.5, 1.0, 0.5], 0.0);
    h.tick();
    assert!(h.world.get::<Fuse>(c).is_some_and(|f| f.0 >= 0), "primed with the short fuse");
}

#[test]
fn lava_lights_a_boat() {
    if !have_data() {
        return;
    }
    let mut h = world();
    h.set_block(0, 1, 0, "minecraft:lava");
    let b = horizon_ecs_proto::boat_state::spawn(&mut h.world, 0, false, [0.5, 1.0, 0.5], 0.0);
    h.tick();
    assert!(h.world.get::<horizon_ecs_proto::boat_state::Hull>(b).is_some_and(|x| x.fire >= 300));
}

fn arrow(h: &mut Herd) -> Entity {
    let e = launch(&mut h.world, Flight::ARROW, Vec3::new(0.5, 1.5, 0.5), Vec3::new(0.0, 0.0, 0.0), None);
    h.tick();
    e
}

#[test]
fn an_arrow_in_lava_ignites_and_water_puts_it_out() {
    if !have_data() {
        return;
    }
    let mut h = world();
    h.set_block(0, 1, 0, "minecraft:lava");
    let e = arrow(&mut h);
    assert!(fire_left(&h.world, e) >= 100, "lit by the lava");
    h.set_block(0, 1, 0, "minecraft:water");
    ignite_projectile(&mut h.world, e, 200);
    h.tick();
    assert_eq!(fire_left(&h.world, e), 0);
}

#[test]
fn an_arrow_in_fire_ignites() {
    if !have_data() {
        return;
    }
    let mut h = world();
    h.set_block(0, 1, 0, "minecraft:fire");
    let e = arrow(&mut h);
    assert!(fire_left(&h.world, e) > 0);
}

#[test]
fn honey_caps_a_fall_and_scales_the_sideways_speed() {
    let (p, v) = (Vec3::new(1.25, 1.5, 0.5), Vec3::new(1.0, -0.5, 0.0));
    assert!(sliding_down([0, 1, 0], p, v, 0.6, false), "box past the cell's side, below its top, falling fast");
    assert!(!sliding_down([0, 1, 0], p, v, 0.6, true), "on the ground");
    assert!(!sliding_down([0, 1, 0], Vec3::new(0.5, 1.5, 0.5), v, 0.6, false), "centred over the block");
    assert!(!sliding_down([0, 1, 0], p, Vec3::new(1.0, -0.1, 0.0), 0.6, false), "too slow a fall");
    let n = slide(v);
    assert!((n.y - (-0.05 - 0.08) * 0.98).abs() < 1e-12);
    assert!((n.x - 0.05 / (0.5 / 0.98 - 0.08)).abs() < 1e-9, "{}", n.x);
}
