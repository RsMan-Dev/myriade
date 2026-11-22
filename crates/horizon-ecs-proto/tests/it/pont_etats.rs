//! The creature states through the bridge (`bridge/host_state.rs`): a fresh creature holds none of the components, a neutral write
//! takes the component away, and the reads answer what was written.
use crate::common::*;
use horizon_client_lib::mobs::world::leaked_test_world_mut;
use horizon_ecs_proto::bridge::host::{EcsWorld, HostSetup};
use horizon_ecs_proto::{custom_name::CustomName, custom_name::NameVisible, gliding::FallFlying, left_handed::LeftHanded, pose_state::PoseOverride};
use horizon_ecs_proto::{no_gravity::NoGravity, stuck::Stuck};

fn setup() -> HostSetup {
    HostSetup { seed: 7, dimension: "overworld", simulation_radius: 4 }
}

fn spawn_cow(h: &mut EcsWorld, mw: &horizon_client_lib::mobs::MobWorld<'_>) -> bevy_ecs::entity::Entity {
    h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species")
}

#[test]
fn a_fresh_creature_holds_no_state() {
    have_data();
    let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    let e = spawn_cow(&mut h, mw);
    let w = h.world();
    assert!(w.get::<CustomName>(e).is_none() && w.get::<NameVisible>(e).is_none());
    assert!(w.get::<NoGravity>(e).is_none() && w.get::<LeftHanded>(e).is_none());
    assert!(w.get::<Stuck>(e).is_none() && w.get::<FallFlying>(e).is_none());
    assert!(w.get::<PoseOverride>(e).is_none());
}

#[test]
fn neutral_writes_take_the_components_away() {
    have_data();
    let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    let e = spawn_cow(&mut h, mw);
    h.set_custom_name(e, Some("Bessie".into()));
    h.set_custom_name(e, None);
    h.set_name_visible(e, true);
    h.set_name_visible(e, false);
    h.set_no_gravity(e, true);
    h.set_no_gravity(e, false);
    h.set_left_handed(e, true);
    h.set_left_handed(e, false);
    h.set_stuck(e, 3, 1);
    h.set_stuck(e, 0, 0);
    h.set_gliding(e, true);
    h.set_gliding(e, false);
    h.set_pose(e, Some(2));
    h.set_pose(e, None);
    let w = h.world();
    assert!(w.get::<CustomName>(e).is_none() && w.get::<NameVisible>(e).is_none());
    assert!(w.get::<NoGravity>(e).is_none() && w.get::<LeftHanded>(e).is_none());
    assert!(w.get::<Stuck>(e).is_none() && w.get::<FallFlying>(e).is_none());
    assert!(w.get::<PoseOverride>(e).is_none());
}

#[test]
fn written_states_read_back() {
    have_data();
    let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    let e = spawn_cow(&mut h, mw);
    h.set_custom_name(e, Some("Bessie".into()));
    assert_eq!(h.custom_name(e).as_deref(), Some("Bessie"));
    h.set_stuck(e, 3, 1);
    let stuck = h.stuck(e);
    assert_eq!((stuck.arrows, stuck.stingers), (3, 1));
    h.set_gliding(e, true);
    assert!(h.world().get::<FallFlying>(e).is_some());
    h.set_pose(e, Some(2));
    assert_eq!(h.pose(e), Some(2));
}
