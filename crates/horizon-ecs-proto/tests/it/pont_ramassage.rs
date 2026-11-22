//! The pickups through the bridge (`bridge/host_pickup.rs`): a player takes the stacks and the orbs in its box as the host accepts them,
//! a stack waits for its pickup delay, the end of life of a stack can be held for the host and extended.
use crate::common::*;
use horizon_client_lib::items::ItemStack;
use horizon_client_lib::mobs::world::leaked_test_world_mut;
use horizon_client_lib::mobs::MobWorld;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::bridge::host::{EcsWorld, HostSetup, PlayerFact};
use horizon_ecs_proto::cow::Clock;
use horizon_ecs_proto::goals::Player;
use horizon_ecs_proto::host_out_items::ItemEvent;
use horizon_ecs_proto::item::{self, Expiring, Item};
use horizon_ecs_proto::orb::{self, Orb};
use horizon_ecs_proto::targeting::Who;

const UUID: [i32; 4] = [1, 2, 3, 4];
const FEET: [f64; 3] = [0.5, 1.0, 0.5];

fn setup() -> HostSetup {
    HostSetup { seed: 7, dimension: "overworld", simulation_radius: 4 }
}

/// Runs `n` ticks of the host: the wheel's far deadlines cascade from level to level as the clock goes by, so it is never jumped.
fn advance(h: &mut EcsWorld, mw: &MobWorld<'_>, n: u64) {
    for _ in 0..n {
        h.step(mw);
    }
}

fn online(h: &mut EcsWorld) {
    h.set_player(Player { id: 1, x: FEET[0], y: FEET[1], z: FEET[2], ..Default::default() }, PlayerFact { alive: true, targetable: true, hero: None });
}

fn all_of(_: &ItemStack) -> u32 {
    u32::MAX
}

#[test]
fn a_stack_in_reach_is_taken_whole_and_the_far_one_stays() {
    have_data();
    let (mw, mut h) = awake_host(&setup());
    let near = item::drop_item(h.world_mut(), ItemStack::new("minecraft:wheat", 3), Vec3::new(0.5, 1.0, 0.5), Vec3::ZERO, 0);
    let far = item::drop_item(h.world_mut(), ItemStack::new("minecraft:wheat", 3), Vec3::new(5.5, 1.0, 0.5), Vec3::ZERO, 0);
    let taken = h.pickup_items(mw, Who::Player(1), UUID, FEET, 1.0, &mut all_of);
    assert_eq!(taken.len(), 1);
    assert_eq!(taken[0].1.count, 3);
    assert!(h.world().get_entity(near).is_err());
    assert!(h.world().get::<Item>(far).is_some());
}

#[test]
fn the_host_takes_what_it_accepts_and_the_rest_stays() {
    have_data();
    let (mw, mut h) = awake_host(&setup());
    let e = item::drop_item(h.world_mut(), ItemStack::new("minecraft:wheat", 3), Vec3::new(0.5, 1.0, 0.5), Vec3::ZERO, 0);
    let taken = h.pickup_items(mw, Who::Player(1), UUID, FEET, 1.0, &mut |_: &ItemStack| 2);
    assert_eq!(taken.len(), 1);
    assert_eq!(taken[0].1.count, 2);
    assert_eq!(h.world().get::<Item>(e).map(|it| it.stack.count), Some(1));
}

#[test]
fn a_stack_targeted_at_another_player_stays() {
    have_data();
    let (mw, mut h) = awake_host(&setup());
    let e = item::drop_item(h.world_mut(), ItemStack::new("minecraft:wheat", 1), Vec3::new(0.5, 1.0, 0.5), Vec3::ZERO, 0);
    h.world_mut().get_mut::<Item>(e).unwrap().target = Some([9, 9, 9, 9]);
    assert!(h.pickup_items(mw, Who::Player(1), UUID, FEET, 1.0, &mut all_of).is_empty());
    assert!(h.pickup_items(mw, Who::Player(1), [9, 9, 9, 9], FEET, 1.0, &mut all_of).len() == 1);
}

#[test]
fn a_stack_waits_for_its_pickup_delay() {
    have_data();
    let (mw, mut h) = awake_host(&setup());
    let e = item::drop_item(h.world_mut(), ItemStack::new("minecraft:wheat", 1), Vec3::new(0.5, 1.0, 0.5), Vec3::ZERO, 10);
    assert!(h.pickup_items(mw, Who::Player(1), UUID, FEET, 1.0, &mut all_of).is_empty());
    h.world_mut().resource_mut::<Clock>().0 += 10;
    assert_eq!(h.pickup_items(mw, Who::Player(1), UUID, FEET, 1.0, &mut all_of).len(), 1);
    assert!(h.world().get_entity(e).is_err());
}

#[test]
fn an_orb_without_an_id_is_bound_and_collected_one_of_its_count_at_a_time() {
    have_data();
    let (mw, mut h) = awake_host(&setup());
    let e = orb::spawn_orb(h.world_mut(), 7, 2, Vec3::new(0.5, 1.0, 0.5), Vec3::ZERO);
    let mut next = 40;
    let mut alloc = || {
        next += 1;
        next
    };
    let found = h.pickup_orbs(mw, FEET, 1.0, &mut alloc);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].1, e);
    assert!(found[0].0 != 0);
    assert_eq!(h.collect_orb(mw, e), Some(7));
    assert_eq!(h.world().get::<Orb>(e).map(|o| o.count), Some(1));
    assert_eq!(h.collect_orb(mw, e), Some(7));
    assert!(h.world().get_entity(e).is_err());
}

#[test]
fn the_thrower_is_kept_on_the_stack() {
    have_data();
    let (mw, mut h) = awake_host(&setup());
    let e = item::drop_item(h.world_mut(), ItemStack::new("minecraft:wheat", 1), Vec3::new(0.5, 1.0, 0.5), Vec3::ZERO, 0);
    h.set_thrower(e, [1, 2, 3, 4]);
    assert_eq!(item::thrower(h.world(), e), Some([1, 2, 3, 4]));
}

#[test]
fn extending_the_life_pushes_the_end_back_by_its_ticks() {
    have_data();
    let (mw, mut h) = awake_host_on_stone(&setup());
    online(&mut h);
    let e = item::drop_item(h.world_mut(), ItemStack::new("minecraft:wheat", 1), Vec3::new(0.5, 1.0, 0.5), Vec3::ZERO, 0);
    advance(&mut h, mw, 100);
    h.extend_item_life(e, 100);
    advance(&mut h, mw, item::LIFE - 1);
    assert!(h.world().get::<Item>(e).is_some());
    advance(&mut h, mw, 1);
    assert!(h.world().get_entity(e).is_err());
}

#[test]
fn with_the_hook_the_end_is_held_and_reported_until_the_host_answers() {
    have_data();
    let (mw, mut h) = awake_host_on_stone(&setup());
    online(&mut h);
    h.set_item_expiry_hooked(true);
    let e = item::drop_item(h.world_mut(), ItemStack::new("minecraft:wheat", 1), Vec3::new(0.5, 1.0, 0.5), Vec3::ZERO, 0);
    advance(&mut h, mw, item::LIFE - 1);
    assert!(h.world().get::<Expiring>(e).is_none());
    let events: Vec<ItemEvent> = h.step(mw).into_iter().filter_map(|ev| if let horizon_ecs_proto::host_out::HostEvent::Item(i) = ev { Some(i) } else { None }).collect();
    assert!(matches!(events.as_slice(), [ItemEvent::Expiring { entity, .. }] if *entity == e));
    assert!(h.world().get::<Expiring>(e).is_some());
    h.resolve_item_expiry(e, Some(50));
    // The end is 50 ticks after the hold began: the step that reported it is the hold's tick.
    advance(&mut h, mw, 49);
    assert!(h.world().get::<Expiring>(e).is_none());
    advance(&mut h, mw, 1);
    assert!(h.world().get::<Expiring>(e).is_some());
    h.resolve_item_expiry(e, None);
    assert!(h.world().get_entity(e).is_err());
}
