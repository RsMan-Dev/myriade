//! The dragon's bridge (lot dragon 1): the fight loads, its tick makes the fight's dragon, and a placed spike crystal is counted and read back.
use crate::common::*;
use myriade_engine_lib::mobs::world::leaked_test_world_mut;
use horizon_ecs_proto::bridge::host::{EcsWorld, HostSetup};
use horizon_ecs_proto::dragon::fight::FightSave;

fn setup() -> HostSetup {
    HostSetup { seed: 7, dimension: "the_end", simulation_radius: 4 }
}

#[test]
fn fight_tick_makes_the_fight_dragon_once_loaded() {
    have_data();
    let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    assert!(!h.fight_view().loaded, "nothing is loaded before fight_load");
    h.fight_load(7, FightSave::default());
    assert!(h.fight_view().loaded);
    h.fight_tick(mw, true);
    assert!(h.fight_view().dragon.is_some(), "a fight with no dragon makes one");
    assert!(h.fight_view().crystals_alive > 0, "the spikes are placed on the first tick");
}

#[test]
fn placed_crystal_is_counted_and_read_back() {
    have_data();
    let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    h.fight_load(7, FightSave::default());
    let before = h.fight_view().crystals_alive;
    let e = h.place_crystal(mw, [10.5, 70.0, 10.5], true);
    assert_eq!(h.fight_view().crystals_alive, before + 1);
    let views = h.crystal_views();
    let found = views.iter().find(|(c, _)| *c == e).expect("the crystal is listed");
    assert_eq!(found.1.position, [10.5, 70.0, 10.5]);
    assert!(found.1.show_bottom);
}
