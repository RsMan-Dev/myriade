//! The carts' contents and their aim through the bridge (`bridge/cart_more.rs`): a slot reads back and an absent slot is refused, the loot
//! is taken once, a ray enters the cart in its path and misses the one beside it, and a box returns only the carts inside it.
use crate::common::*;
use bevy_ecs::entity::Entity;
use horizon_client_lib::items::ItemStack;
use horizon_client_lib::mobs::world::leaked_test_world_mut;
use horizon_core::geometry::Vec3;
use horizon_core::nbt::Tag;
use horizon_ecs_proto::bridge::host::{EcsWorld, HostSetup};
use horizon_ecs_proto::cart_state::Contents;

const CHEST_CART: &str = "minecraft:chest_minecart";

fn setup() -> HostSetup {
    HostSetup { seed: 7, dimension: "overworld", simulation_radius: 4 }
}

/// A fresh world with a chest cart at (0.5, 1, 0.5), the one under test, and one at (5.5, 1, 5.5), both spawned into the spatial index.
fn fixture() -> (EcsWorld, Entity, Entity) {
    have_data();
    let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    let a = h.spawn_cart(mw, CHEST_CART, [0.5, 1.0, 0.5], 0.0).expect("a known genre");
    let b = h.spawn_cart(mw, CHEST_CART, [5.5, 1.0, 5.5], 0.0).expect("a known genre");
    (h, a, b)
}

#[test]
fn a_slot_reads_back_and_an_absent_slot_is_refused() {
    let (mut h, e, _) = fixture();
    assert!(h.cart_set_slot(e, 0, Some(ItemStack::new("minecraft:stone", 3))));
    assert_eq!(h.cart_items(e).and_then(|s| s[0].clone()), Some(ItemStack::new("minecraft:stone", 3)));
    assert!(!h.cart_set_slot(e, 27, Some(ItemStack::new("minecraft:stone", 1))));
}

#[test]
fn the_loot_is_taken_once() {
    let (mut h, e, _) = fixture();
    h.world_mut().get_mut::<Contents>(e).expect("a chest cart has contents").loot = Some(horizon_client_render::chunk::PendingContainerLoot {
        table: Tag::String("minecraft:chests/simple_dungeon".into()),
        seed: Some(Tag::Long(42)),
    });
    assert_eq!(h.cart_take_loot(e), Some(("minecraft:chests/simple_dungeon".to_string(), 42)));
    assert_eq!(h.cart_take_loot(e), None);
}

#[test]
fn a_ray_enters_the_cart_in_its_path_and_misses_the_one_beside_it() {
    let (h, e, _) = fixture();
    let (hit, t) = h.aimed_cart(Vec3::new(0.5, 1.2, -2.0), Vec3::new(0.0, 0.0, 1.0), 5.0, None).expect("the cart is in the path");
    assert_eq!(hit, e);
    assert!(t > 2.0 && t < 2.5, "the box starts half a block-width before z = 0.5, at t in (2.0, 2.5): {t}");
    assert!(h.aimed_cart(Vec3::new(-2.0, 1.2, 5.0), Vec3::new(1.0, 0.0, 0.0), 5.0, None).is_none());
    assert!(h.aimed_cart(Vec3::new(0.5, 1.2, -2.0), Vec3::new(0.0, 0.0, 1.0), 5.0, Some(e)).is_none());
}

#[test]
fn a_box_returns_the_carts_inside_it_only() {
    let (h, inside, _) = fixture();
    let found = h.carts_in(Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 2.0, 1.0));
    assert_eq!(found, vec![inside]);
}
