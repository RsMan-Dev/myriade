//! The block factors of `block_factor`: soul sand slows, honey halves the jump, and a `BlockSpeed` cart is slowed on soul sand as
//! `Entity.move` scales its horizontal velocity. The expected values are vanilla's (soul sand 0.4, honey jump 0.5, the others 1.0).
use bevy_ecs::prelude::Entity;
use myriade_engine_lib::mobs::MobWorld;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::block_factor::{self, BlockSpeed};
use horizon_ecs_proto::cart_move::{self, TrackRead};
use horizon_ecs_proto::cart_state::Cart;
use horizon_ecs_proto::cow::{Herd, Level, Pos, Vel};
use horizon_ecs_proto::minecart::{Kind, Track};

fn herd() -> Herd {
    Herd::with_slab(4, false, false)
}

fn mw(h: &Herd) -> &'static MobWorld<'static> {
    h.world.resource::<Level>().mw.0
}

fn cart(h: &mut Herd, at: Vec3, block_speed: bool) -> Entity {
    let e = h.world.spawn((Cart, Kind::Rideable, Track::default(), Pos(at), Vel(Vec3::ZERO))).id();
    if block_speed {
        h.world.entity_mut(e).insert(BlockSpeed);
    }
    e
}

fn speed_of(h: &Herd, e: Entity) -> f64 {
    let v = cart_move::velocity(&h.world, e).expect("a cart has a velocity");
    (v[0] * v[0] + v[2] * v[2]).sqrt()
}

#[test]
fn soul_sand_slows_the_point_above_it() {
    let mut h = herd();
    h.set_block(1, 0, 0, "minecraft:soul_sand");
    h.tick();
    assert!((block_factor::speed(mw(&h), [1.5, 1.0, 0.5]) - 0.4).abs() < 1e-6);
}

#[test]
fn honey_halves_the_jump_of_its_block() {
    let mut h = herd();
    h.set_block(2, 1, 2, "minecraft:honey_block");
    h.tick();
    assert!((block_factor::jump(mw(&h), [2.5, 1.0, 2.5]) - 0.5).abs() < 1e-6);
}

#[test]
fn stone_does_not_slow() {
    let mut h = herd();
    h.set_block(3, 0, 0, "minecraft:stone");
    h.tick();
    assert_eq!(block_factor::speed(mw(&h), [3.5, 1.0, 0.5]), 1.0);
}

#[test]
fn nothing_slows_outside_the_loaded_chunks() {
    let h = herd();
    assert_eq!(block_factor::speed(mw(&h), [1000.5, 1.0, 0.5]), 1.0);
}

#[test]
fn a_block_speed_cart_on_soul_sand_is_slower_than_an_unmarked_one() {
    let mut h = herd();
    // No rail anywhere: the carts are off the track, where the block factor applies.
    h.world.insert_resource(TrackRead { rail: |_, _| None, conductor: |_, _| false, activator: |_, _| false, signal: |_, _| 0 });
    h.set_block(-2, 0, 0, "minecraft:soul_sand");
    h.set_block(1, 0, 0, "minecraft:soul_sand");
    h.tick();
    let slow = cart(&mut h, Vec3::new(-1.5, 1.0, 0.5), true);
    let free = cart(&mut h, Vec3::new(1.5, 1.0, 0.5), false);
    for e in [slow, free] {
        cart_move::push(&mut h.world, e, [0.2, 0.0, 0.0]);
    }
    h.tick();
    let (slow_speed, free_speed) = (speed_of(&h, slow), speed_of(&h, free));
    assert!(slow_speed < free_speed);
    assert!((slow_speed / free_speed - 0.4).abs() < 1e-6);
}
