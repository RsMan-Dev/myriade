//! The item use through the bridge (`bridge/host_using.rs`): a use starts only with an item in the hand, its ticks are the host's, the totem
//! and the draws answer, and nothing is written on a creature that uses nothing.
use crate::common::*;
use bevy_ecs::entity::Entity;
use myriade_engine_lib::items::ItemStack;
use myriade_engine_lib::mobs::world::leaked_test_world_mut;
use horizon_ecs_proto::bridge::host::{EcsWorld, HostSetup};
use horizon_ecs_proto::cow::{Burning, Health};
use horizon_ecs_proto::host_out_using::UsingEvent;
use horizon_ecs_proto::using::{self, UsingItem};

fn setup() -> HostSetup {
    HostSetup { seed: 7, dimension: "overworld", simulation_radius: 4 }
}

/// The using events the channel carries since the last step.
fn events(h: &mut EcsWorld, mw: &myriade_engine_lib::mobs::MobWorld<'_>) -> Vec<UsingEvent> {
    use horizon_ecs_proto::host_out::HostEvent;
    h.step(mw).into_iter().filter_map(|ev| if let HostEvent::Using(u) = ev { Some(u) } else { None }).collect()
}

/// Two cows in a fresh world; the first is the one under test.
fn fixture() -> (EcsWorld, Entity, Entity, &'static myriade_engine_lib::mobs::MobWorld<'static>) {
    have_data();
    let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    let a = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    let b = h.spawn(mw, "minecraft:cow", [4.5, 1.0, 0.5], None).expect("a known species");
    (h, a, b, mw)
}

#[test]
fn a_use_reads_back_and_refuses_a_second_start() {
    let (mut h, e, _, _mw) = fixture();
    h.set_equipment_stack(e, 0, ItemStack::new("minecraft:bow", 1));
    assert!(h.start_using(e, false));
    h.set_item_use_remaining(e, 40);
    assert_eq!(h.using(e), Some((false, 40)));
    assert!(!h.start_using(e, false));
}

#[test]
fn an_older_deadline_does_not_end_a_later_use() {
    let (mut h, e, _, _mw) = fixture();
    h.set_equipment_stack(e, 0, ItemStack::new("minecraft:bow", 1));
    h.start_using(e, false);
    h.set_item_use_remaining(e, 40);
    h.set_item_use_remaining(e, 100);
    // The clock is still at its start: neither deadline is due, so the use stays, and the rule on `until` holds for both.
    using::tick_down(h.world_mut(), e);
    assert!(h.is_using_item(e));
    assert_eq!(h.world().get::<UsingItem>(e).map(|u| u.until), Some(100));
}

#[test]
fn an_empty_hand_does_not_start_a_use() {
    let (mut h, e, _, _mw) = fixture();
    assert!(!h.start_using(e, true));
    assert!(!h.is_using_item(e));
}

#[test]
fn stop_removes_the_component_and_says_nothing() {
    let (mut h, e, _, mw) = fixture();
    h.set_equipment_stack(e, 0, ItemStack::new("minecraft:bow", 1));
    h.start_using(e, false);
    h.stop_using(e);
    assert!(h.world().get::<UsingItem>(e).is_none());
    assert!(events(&mut h, mw).is_empty(), "the host's own call did the effect");
}

#[test]
fn release_ends_the_use() {
    let (mut h, e, _, mw) = fixture();
    h.set_equipment_stack(e, 0, ItemStack::new("minecraft:bow", 1));
    h.start_using(e, false);
    h.set_item_use_remaining(e, 12);
    h.release_using_item(e);
    assert!(events(&mut h, mw).is_empty());
    assert!(!h.is_using_item(e));
}

#[test]
fn ignite_sets_the_fire() {
    let (mut h, e, _, _mw) = fixture();
    h.ignite(e, 100);
    assert!(h.world().get::<Burning>(e).is_some());
}

#[test]
fn a_totem_in_the_off_hand_saves_the_creature() {
    let (mut h, e, _, mw) = fixture();
    h.set_equipment_stack(e, 1, ItemStack::new("minecraft:totem_of_undying", 1));
    assert!(h.try_totem(e));
    assert_eq!(h.world().get::<Health>(e).map(|x| x.0), Some(1.0));
    assert!(h.equipment_stack(e, 1).is_none());
    assert!(events(&mut h, mw).iter().any(|x| matches!(x, UsingEvent::TotemPopped { off_hand: true, .. })));
}

#[test]
fn without_a_totem_nothing_happens() {
    let (mut h, e, _, _mw) = fixture();
    assert!(!h.try_totem(e));
}

#[test]
fn draws_stay_in_bounds_and_differ_between_creatures() {
    let (mut h, a, b, _mw) = fixture();
    for _ in 0..50 {
        let v = h.next_int_bound(a, 10).expect("a live creature");
        assert!((0..10).contains(&v));
    }
    assert_ne!(h.next_float(a), h.next_float(b));
    assert!(!h.fire_immune(a) && !h.is_roaring(a));
    assert!(h.world().get::<UsingItem>(b).is_none());
}
