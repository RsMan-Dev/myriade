//! The conversion the host may refuse: unwatched it is immediate; watched it is asked, and both entities live until the host finishes it.
use crate::common::*;
use horizon_client_lib::mobs::world::leaked_test_world_mut;
use horizon_ecs_proto::bridge::host::{EcsWorld, HostSetup};
use horizon_ecs_proto::conversion::{ConvertKeep, convert_or_ask};
use horizon_ecs_proto::host_out::HostEvent;

macro_rules! world {
    ($h:ident, $mw:ident) => {
        have_data();
        let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
        let ($mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
        let mut $h = EcsWorld::new(&HostSetup { seed: 7, dimension: "overworld", simulation_radius: 4 }, $mw, types, &tags);
    };
}

#[test]
fn a_watched_conversion_waits_for_the_host_and_can_be_refused() {
    world!(h, mw);
    h.set_conversion_watched(true);
    let e = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    assert!(convert_or_ask(h.world_mut(), e, "minecraft:pig", ConvertKeep::NONE));
    assert!(h.world().get_entity(e).is_ok(), "nothing changed before the answer");
    let asked = h.step(mw).into_iter().filter(|ev| matches!(ev, HostEvent::ConversionAsk { entity, .. } if *entity == e)).count();
    assert_eq!(asked, 1);
    assert!(h.resolve_conversion(e, true).is_none(), "a refusal makes no new entity");
    assert!(h.world().get_entity(e).is_ok(), "the refused entity stays");
}

#[test]
fn a_watched_conversion_keeps_the_old_entity_until_it_is_finished() {
    world!(h, mw);
    h.set_conversion_watched(true);
    let e = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    convert_or_ask(h.world_mut(), e, "minecraft:pig", ConvertKeep::NONE);
    let n = h.resolve_conversion(e, false).expect("the new entity is born");
    assert!(h.world().get_entity(e).is_ok() && h.world().get_entity(n).is_ok(), "both live for the post hook");
    h.finish_conversion(e);
    assert!(h.world().get_entity(e).is_err(), "the old entity leaves");
}

#[test]
fn an_unwatched_conversion_is_immediate() {
    world!(h, mw);
    let e = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    assert!(convert_or_ask(h.world_mut(), e, "minecraft:pig", ConvertKeep::NONE));
    assert!(h.world().get_entity(e).is_err(), "no question when nobody listens");
}
