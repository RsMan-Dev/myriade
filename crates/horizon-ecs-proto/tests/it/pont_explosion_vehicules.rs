//! A blast over the bridge reaches the boats and carts of the ECS (`explosion::blast_others`): a near boat breaks, a far cart is pushed
//! and kept, a TNT cart primes instead of breaking, a mob's blast spares the vehicles when mob griefing is off, and the snapshot of the
//! targets keeps visiting the rest when one is gone. The box query finds a vehicle whose box reaches in though its position is outside.
use crate::common::*;
use bevy_ecs::prelude::*;
use horizon_client_lib::mobs::{MobTypes, MobWorld, world::leaked_test_world_mut};
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::bridge::host::{EcsWorld, HostSetup, PlayerFact};
use horizon_ecs_proto::cart_state::Cart;
use horizon_ecs_proto::goals::Player;
use horizon_ecs_proto::host_out_cart_variants::{CartEvent, CartOut};
use horizon_ecs_proto::projectile::Blast;
use std::sync::Arc;

const BOAT: &str = "oak";
const TNT_CART: &str = "minecraft:tnt_minecart";
const CHEST_CART: &str = "minecraft:chest_minecart";
const RADIUS: f32 = 4.0;

fn setup() -> HostSetup {
    HostSetup { seed: 7, dimension: "overworld", simulation_radius: 4 }
}

fn fact() -> PlayerFact {
    PlayerFact { alive: true, targetable: true, ..Default::default() }
}

/// Away from the blast (more than its diameter) and near enough to keep the vehicles awake.
fn player() -> Player {
    Player { id: 1, x: 10.5, y: 1.0, z: 0.5, ..Default::default() }
}

fn blast(source: Option<Entity>) -> Blast {
    Blast { at: Vec3::new(0.5, 1.0, 0.5), radius: RADIUS, fire: false, owner: None, source }
}

fn stone_world() -> (&'static MobWorld<'static>, &'static MobTypes) {
    let (mw, types, _) = leaked_test_world_mut(true, |blocks, registry| {
        let stone = registry.default_state("minecraft:stone").unwrap();
        for cx in -4..4 {
            for cz in -4..4 {
                blocks.chunks.insert((cx, cz), Arc::new(horizon_client_render::chunk::Chunk::empty(cx, cz)));
            }
        }
        for x in -20..20 {
            for z in -20..20 {
                for y in -2..=0 {
                    blocks.set(x, y, z, stone).unwrap();
                }
            }
        }
    })
    .expect("HORIZON_TEST_RESOURCES");
    (mw, types)
}

fn tags() -> std::path::PathBuf {
    std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"))
}

fn world() -> (EcsWorld, &'static MobWorld<'static>) {
    have_data();
    let (mw, types) = stone_world();
    let mut h = EcsWorld::new(&setup(), mw, types, &tags());
    h.set_player(player(), fact());
    h.step(mw);
    h.step(mw);
    (h, mw)
}

fn gone(h: &EcsWorld, e: Entity) -> bool {
    h.world().get_entity(e).is_err()
}

#[test]
fn a_blast_breaks_a_near_boat() {
    let (mut h, mw) = world();
    let boat = h.spawn_boat(mw, BOAT, false, [1.5, 1.0, 0.5], 0.0).expect("a known wood");
    h.explode_with(mw, blast(None), false);
    assert!(gone(&h, boat), "the damage past 40 breaks the boat");
}

#[test]
fn a_blast_pushes_a_far_cart_and_keeps_it() {
    let (mut h, mw) = world();
    let cart = h.spawn_cart(mw, CHEST_CART, [8.1, 1.0, 0.5], 0.0).expect("a known genre");
    h.explode_with(mw, blast(None), false);
    assert!(!gone(&h, cart), "a blow at the edge hurts without breaking");
    let v = h.velocity(cart).expect("the push gave it a velocity");
    assert!(v.x > 0.0, "pushed away from the centre, got {v:?}");
}

#[test]
fn a_blast_primes_a_tnt_cart_instead_of_breaking_it() {
    let (mut h, mw) = world();
    let cart = h.spawn_cart(mw, TNT_CART, [1.5, 1.0, 0.5], 0.0).expect("a known genre");
    h.explode_with(mw, blast(None), false);
    assert!(!gone(&h, cart), "a TNT cart under an explosion primes, it does not break");
    let primed = h.world().get_resource::<CartOut>().is_some_and(|o| o.0.iter().any(|ev| matches!(ev, CartEvent::Fuse { entity } if *entity == cart)));
    assert!(primed, "the host is told the fuse is lit");
}

#[test]
fn a_blast_breaks_two_vehicles_and_still_pushes_the_next() {
    let (mut h, mw) = world();
    let a = h.spawn_boat(mw, BOAT, false, [1.5, 1.0, 0.5], 0.0).expect("a known wood");
    let b = h.spawn_boat(mw, BOAT, false, [0.5, 1.0, 1.5], 0.0).expect("a known wood");
    let far = h.spawn_cart(mw, CHEST_CART, [8.1, 1.0, 0.5], 0.0).expect("a known genre");
    h.explode_with(mw, blast(None), false);
    assert!(gone(&h, a) && gone(&h, b), "both near boats break");
    assert!(h.velocity(far).is_some_and(|v| v.x > 0.0), "the next vehicle is pushed, its identity unchanged");
}

#[test]
fn a_target_removed_before_its_turn_leaves_the_others_visited() {
    let (mut h, mw) = world();
    let first = h.spawn_boat(mw, BOAT, false, [1.5, 1.0, 0.5], 0.0).expect("a known wood");
    let next = h.spawn_boat(mw, BOAT, false, [0.5, 1.0, 1.5], 0.0).expect("a known wood");
    let last = h.spawn_cart(mw, CHEST_CART, [8.1, 1.0, 0.5], 0.0).expect("a known genre");
    // The index still lists `next` for the blast though it is gone: the walk goes on over the originals left.
    h.world_mut().despawn(next);
    h.explode_with(mw, blast(None), false);
    assert!(gone(&h, first), "the first target is hit");
    assert!(h.velocity(last).is_some_and(|v| v.x > 0.0), "the last one is still visited");
}

#[test]
fn a_box_finds_a_vehicle_whose_position_is_outside_it() {
    let (mut h, mw) = world();
    let boat = h.spawn_boat(mw, BOAT, false, [2.0, 1.0, 0.5], 0.0).expect("a known wood");
    let (min, max) = (Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.5, 3.0, 1.0));
    assert!(!h.boats_in(min, max).contains(&boat), "the position test misses it");
    assert!(h.vehicles_touching(min, max).contains(&boat), "its box reaches into the query box");
    let cart = h.spawn_cart(mw, CHEST_CART, [20.0, 1.0, 0.5], 0.0).expect("a known genre");
    assert!(!h.vehicles_touching(min, max).contains(&cart), "a cart far away is left out");
    assert!(h.world().get::<Cart>(cart).is_some());
}

#[test]
fn a_wind_burst_pushes_a_cart_at_rest() {
    let (mut h, mw) = world();
    let cart = h.spawn_cart(mw, CHEST_CART, [2.5, 1.0, 0.5], 0.0).expect("a known genre");
    h.lent(mw, |w| horizon_ecs_proto::explosion::wind_burst(w, Vec3::new(0.5, 1.0, 0.5), 3.0, None));
    assert!(h.velocity(cart).is_some_and(|v| v.x > 0.0), "the burst gives the cart a velocity away from the centre");
}

#[test]
fn a_routed_blast_leaves_the_boat_blow_to_the_host() {
    let (mut h, mw) = world();
    let boat = h.spawn_boat(mw, BOAT, false, [1.5, 1.0, 0.5], 0.0).expect("a known wood");
    h.route_boat_blows(true);
    h.explode_with(mw, blast(None), false);
    assert!(!gone(&h, boat), "the host plays the hit, P does not");
    let blows = h.take_boat_blows();
    assert!(matches!(blows.as_slice(), [(e, d, _)] if *e == boat && *d > 40.0), "the blow is queued with its damage: {blows:?}");
}
