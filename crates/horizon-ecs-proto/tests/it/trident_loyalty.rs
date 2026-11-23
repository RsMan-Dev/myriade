//! The Loyalty return of a thrown trident to its owner (a player, a creature) and the Channeling strike on a creature with no storm, on the
//! host's world. Written, not run.
use crate::common::*;
use bevy_ecs::prelude::*;
use myriade_engine_lib::items::ItemStack;
use myriade_engine_lib::mobs::world::leaked_test_world_mut;
use myriade_engine_lib::mobs::MobWorld;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::bridge::host::{EcsWorld, HostSetup, PlayerFact};
use horizon_ecs_proto::goals::Player;
use horizon_ecs_proto::host_out_trident::TridentEvent;
use horizon_ecs_proto::targeting::Who;
use horizon_ecs_proto::trident::{throw, Pickup, Trident};

fn setup() -> HostSetup {
    HostSetup { seed: 7, dimension: "overworld", simulation_radius: 4 }
}

fn with_mw() -> (&'static MobWorld<'static>, EcsWorld) {
    let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    (mw, EcsWorld::new(&setup(), mw, types, &tags))
}

fn online(h: &mut EcsWorld, id: u32, x: f64, y: f64, z: f64) {
    h.set_player(Player { id, x, y, z, ..Default::default() }, PlayerFact { alive: true, targetable: true, hero: None });
}

#[test]
fn a_loyal_trident_goes_back_to_its_player_owner_and_is_taken() {
    if !have_data() {
        return;
    }
    let (mw, mut h) = with_mw();
    // The eye of player 1 (y + 1.62) stands on the trident: the return reaches it at once.
    online(&mut h, 1, 0.5, 0.28, 0.5);
    let at = Vec3::new(0.5, 1.9, 0.5);
    let e = throw(h.world_mut(), ItemStack::new("minecraft:trident", 1), at, Vec3::new(0.0, 0.0, 0.0), Some(Who::Player(1)), 1, Pickup::Allowed);
    h.world_mut().get_mut::<Trident>(e).unwrap().dealt = true;
    let events = h.step(mw).into_iter().filter_map(|ev| if let horizon_ecs_proto::host_out::HostEvent::Trident(t) = ev { Some(t) } else { None }).collect::<Vec<TridentEvent>>();
    assert!(h.world().get_entity(e).is_err(), "the trident is gone from the world");
    assert!(events.iter().any(|ev| matches!(ev, TridentEvent::PickedUp { by: Who::Player(1), .. })), "player 1 took it");
    assert!(events.iter().any(|ev| matches!(ev, TridentEvent::Returned { owner: Some(Who::Player(1)), .. })), "it returned to player 1");
}

#[test]
fn a_loyal_trident_goes_back_to_its_creature_owner_and_is_gone() {
    if !have_data() {
        return;
    }
    let (mw, mut h) = with_mw();
    // A player far away keeps the schedule running.
    online(&mut h, 2, 40.0, 1.0, 40.0);
    let at = [0.5, 64.0, 0.5];
    let owner = h.spawn(mw, "minecraft:pig", at, None).expect("a known species");
    let e = throw(h.world_mut(), ItemStack::new("minecraft:trident", 1), Vec3::new(at[0], at[1], at[2]), Vec3::new(0.0, 0.0, 0.0), Some(Who::Entity(owner)), 1, Pickup::Allowed);
    h.world_mut().get_mut::<Trident>(e).unwrap().dealt = true;
    let events = h.step(mw).into_iter().filter_map(|ev| if let horizon_ecs_proto::host_out::HostEvent::Trident(t) = ev { Some(t) } else { None }).collect::<Vec<TridentEvent>>();
    assert!(h.world().get_entity(e).is_err(), "the trident is gone from the world");
    assert!(events.iter().any(|ev| matches!(ev, TridentEvent::Returned { owner: Some(Who::Entity(o)), .. } if *o == owner)), "it returned to the creature");
}

#[test]
fn a_channeling_blow_without_a_storm_strikes_no_lightning() {
    if !have_data() {
        return;
    }
    let (mw, mut h) = with_mw();
    online(&mut h, 2, 40.0, 1.0, 40.0);
    let pig = h.spawn(mw, "minecraft:pig", [0.5, 64.0, 2.0], None).expect("a known species");
    let e = throw(h.world_mut(), ItemStack::new("minecraft:trident", 1), Vec3::new(0.5, 64.5, 0.5), Vec3::new(0.0, 0.0, 1.0), None, 0, Pickup::Allowed);
    h.world_mut().get_mut::<Trident>(e).unwrap().channeling = true;
    let (mut hit, mut thunder) = (false, false);
    for _ in 0..20 {
        for ev in h.step(mw).into_iter().filter_map(|ev| if let horizon_ecs_proto::host_out::HostEvent::Trident(t) = ev { Some(t) } else { None }).collect::<Vec<TridentEvent>>() {
            match ev {
                TridentEvent::Hit { victim, .. } if victim == Who::Entity(pig) => hit = true,
                TridentEvent::Thunder { .. } => thunder = true,
                _ => {}
            }
        }
    }
    assert!(hit, "the trident hit the pig");
    assert!(!thunder, "no storm, no lightning");
}
