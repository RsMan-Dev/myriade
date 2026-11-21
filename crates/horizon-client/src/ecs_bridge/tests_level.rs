//! The level's inert answers, and the host events' mapping by network id (written, not run).
use super::{events::to_mob_event, level::EcsLevel};
use crate::lan::entity::EntityState;
use crate::mobs::{MobEvent, Who};
use bevy_ecs::{entity::Entity, world::World};
use horizon_ecs_proto::{bridge::ids::bind, host_out::HostEvent};

#[test]
fn inert_level_answers_as_an_empty_world() {
    let mut l = EcsLevel::inert();
    let e = Entity::from_raw_u32(1).unwrap();
    let mut st = EntityState::default();
    let mut meta = Vec::new();
    assert!(!l.is_active());
    assert!(l.world().is_none());
    assert!(l.entity_of(1).is_none());
    assert!(!l.exists(1));
    assert_eq!(l.kind(1), None);
    assert_eq!(l.position(1), None);
    assert!(l.ids_near([0.0; 3], 100.0).is_empty());
    assert!(l.drain_modified().is_empty());
    assert!(l.save_chunk(0, 0).is_empty());
    assert!(l.entity_nbt(e).is_none());
    assert!(!l.view_state(e, 1, &mut st));
    l.metadata(e, &mut meta);
    assert!(meta.is_empty());
}

#[test]
fn entity_events_map_to_their_mob_event_by_network_id() {
    let mut world = World::new();
    let e = world.spawn_empty().id();
    assert!(bind(&mut world, e, 7));
    let chest = HostEvent::ChestOpen { chest: [1, 2, 3], open: true, entity: e };
    assert!(matches!(to_mob_event(&world, chest), Some(MobEvent::ChestOpen { mob: 7, open: true, .. })));
    let pull = HostEvent::PullEntity { entity: e, velocity: [0.0, 1.0, 0.0] };
    assert!(matches!(to_mob_event(&world, pull), Some(MobEvent::Push { target: Who::Mob(7), .. })));
}

#[test]
fn an_unbound_entity_or_a_host_only_event_is_kept() {
    let mut world = World::new();
    let e = world.spawn_empty().id();
    assert!(to_mob_event(&world, HostEvent::PullEntity { entity: e, velocity: [0.0; 3] }).is_none());
    assert!(to_mob_event(&world, HostEvent::Pearl { player: 1, to: [0.0; 3], damage: 0.0, endermite: false }).is_none());
}
