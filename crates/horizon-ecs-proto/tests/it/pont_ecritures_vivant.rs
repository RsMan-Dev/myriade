//! The host's writes on a living entity (lot 43): health, turn, motion, fire, effects. Each write is read back through the world.
use crate::common::*;
use bevy_ecs::entity::Entity;
use horizon_client_lib::mobs::world::leaked_test_world_mut;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::bridge::host::{EcsWorld, HostSetup};
use horizon_ecs_proto::cow::{Contact, Health, MaxHealth, Resist, Vel, Yaw};
use horizon_ecs_proto::effects::Kind;
use horizon_ecs_proto::goals::{self, Head};

fn setup() -> HostSetup {
    HostSetup { seed: 7, dimension: "overworld", simulation_radius: 4 }
}

fn world_and_cow() -> (EcsWorld, Entity) {
    have_data();
    let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    let e = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    (h, e)
}

#[test]
fn heal_stops_at_max_and_refuses_the_dead() {
    let (mut h, e) = world_and_cow();
    let max = h.world().get::<MaxHealth>(e).expect("a cow has a max health").0;
    h.set_health(e, 1.0);
    assert!(h.heal(e, 1000.0));
    assert_eq!(h.world().get::<Health>(e).unwrap().0, max, "the heal is capped at the max");
    h.set_health(e, 0.0);
    assert!(!h.heal(e, 1.0), "a dead entity takes no heal");
    assert_eq!(h.world().get::<Health>(e).unwrap().0, 0.0);
}

#[test]
fn set_health_is_bounded() {
    let (mut h, e) = world_and_cow();
    let max = h.world().get::<MaxHealth>(e).unwrap().0;
    assert!(h.set_health(e, -3.0));
    assert_eq!(h.world().get::<Health>(e).unwrap().0, 0.0);
    assert!(h.set_health(e, 1.0e9));
    assert_eq!(h.world().get::<Health>(e).unwrap().0, max);
}

#[test]
fn set_rotation_turns_and_marks_the_entity() {
    let (mut h, e) = world_and_cow();
    h.drain_modified();
    h.set_rotation(e, 90.0, 10.0);
    assert_eq!(h.world().get::<Yaw>(e).unwrap().0, 90.0);
    if let Some(head) = h.world().get::<Head>(e) {
        assert_eq!((head.yaw, head.pitch), (90.0, 10.0));
    }
    assert!(h.drain_modified().contains(&e), "the view reads the new turn");
}

#[test]
fn teleport_motion_keeps_only_the_guarded_axes() {
    let (mut h, e) = world_and_cow();
    goals::set_velocity(h.world_mut(), e, Vec3::new(1.0, 2.0, 3.0));
    h.teleport_motion(e, [true, false, true]);
    let v = h.world().get::<Vel>(e).unwrap().0;
    assert_eq!((v.x, v.y, v.z), (1.0, 0.0, 3.0));
}

#[test]
fn push_adds_to_the_velocity() {
    let (mut h, e) = world_and_cow();
    goals::set_velocity(h.world_mut(), e, Vec3::new(1.0, 0.0, 0.0));
    h.push(e, [1.0, 2.0, 3.0]);
    let v = h.world().get::<Vel>(e).unwrap().0;
    assert_eq!((v.x, v.y, v.z), (2.0, 2.0, 3.0));
}

#[test]
fn knockback_is_nil_with_full_resistance() {
    let (mut h, e) = world_and_cow();
    h.world_mut().entity_mut(e).insert(Resist(1.0));
    goals::set_velocity(h.world_mut(), e, Vec3::new(0.0, 0.0, 0.0));
    h.knockback(e, 0.4, 1.0, 0.0);
    let v = h.world().get::<Vel>(e).unwrap().0;
    assert_eq!((v.x, v.y, v.z), (0.0, 0.0, 0.0));
}

#[test]
fn jump_only_from_the_ground() {
    let (mut h, e) = world_and_cow();
    h.world_mut().entity_mut(e).insert(Contact { on_ground: false, horizontal: false, fall_distance: 0.0, stuck: 0 });
    assert!(!h.jump(e), "no jump in the air");
    h.world_mut().entity_mut(e).insert(Contact { on_ground: true, horizontal: false, fall_distance: 0.0, stuck: 0 });
    assert!(h.jump(e), "a jump from the ground");
    assert!(h.world().get::<Vel>(e).unwrap().0.y > 0.0, "the jump lifts the entity");
}

#[test]
fn set_on_fire_is_read_back_as_a_flag() {
    let (mut h, e) = world_and_cow();
    h.set_on_fire(e, 100);
    assert_eq!(h.flag(e, "on_fire"), Some(true));
}

#[test]
fn add_then_remove_an_effect() {
    let (mut h, e) = world_and_cow();
    assert!(h.add_effect(e, Kind::Poison, 0, 100, 0));
    assert!(h.has_effect(e, Kind::Poison));
    assert!(h.remove_effect(e, Kind::Poison));
    assert!(!h.has_effect(e, Kind::Poison));
}

#[test]
fn retain_effects_keeps_only_the_list() {
    let (mut h, e) = world_and_cow();
    h.add_effect(e, Kind::Speed, 0, 100, 0);
    h.add_effect(e, Kind::Poison, 0, 100, 0);
    assert!(h.retain_effects(e, &[Kind::Speed]));
    assert!(h.has_effect(e, Kind::Speed));
    assert!(!h.has_effect(e, Kind::Poison));
    assert!(!h.retain_effects(e, &[Kind::Speed]), "nothing left to remove");
}
