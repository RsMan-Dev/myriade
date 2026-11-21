//! A raider that joins a raid in the world (loaded with a `RaidId`, or a patrol raider that walks into a raid) tells the host once, and the
//! raid buffs of a wave are worn from the entity's own draw.
use crate::common::*;
use horizon_client_lib::mobs::world::leaked_test_world_mut;
use horizon_core::nbt::{Compound, Tag};
use horizon_ecs_proto::bridge::host::{EcsWorld, HostSetup};
use horizon_ecs_proto::host_out::HostEvent;
use horizon_ecs_proto::raid_buffs::{RaidBuff, RaidBuffRules};
use horizon_ecs_proto::raids::{self, RaidView};

fn world() -> (&'static horizon_client_lib::mobs::world::MobWorld<'static>, EcsWorld) {
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let h = EcsWorld::new(&HostSetup { seed: 7, dimension: "overworld", simulation_radius: 4 }, mw, types, &tags);
    (mw, h)
}

const RULES: RaidBuffRules = RaidBuffRules { odds: 1.0, easy_groups: 3, normal_groups: 5 };

#[test]
fn a_loaded_raider_is_announced_once_without_recounting_health() {
    have_data();
    let (mw, mut h) = world();
    let e = h.spawn(mw, "minecraft:vindicator", [0.5, 1.0, 0.5], None).expect("a known species");
    let id = h.id_for(e, &mut || 41);
    let mut patch = Compound::new();
    patch.insert("RaidId".into(), Tag::Int(9));
    patch.insert("Wave".into(), Tag::Int(2));
    assert!(horizon_ecs_proto::nbt::merge(h.world_mut(), e, &patch));
    let first = h.step(mw);
    assert!(
        first.iter().any(|ev| matches!(ev, HostEvent::RaiderJoined { raid: 9, id: i, wave: 2, loaded: true, .. } if *i == id)),
        "the loaded raider says its raid"
    );
    let again = h.step(mw);
    assert!(again.iter().all(|ev| !matches!(ev, HostEvent::RaiderJoined { .. })), "once only");
}

#[test]
fn an_idle_raider_that_can_join_walks_into_the_active_raid() {
    have_data();
    let (mw, mut h) = world();
    let e = h.spawn(mw, "minecraft:pillager", [0.5, 1.0, 0.5], None).expect("a known species");
    let id = h.id_for(e, &mut || 42);
    assert!(raids::set_can_join_raid(h.world_mut(), e, true));
    h.set_raid_view(3, Some(RaidView { center: [0, 1, 0], active: true, over: false, loss: false, wave: 4 }));
    let mut joined = false;
    for _ in 0..40 {
        joined |= h.step(mw).iter().any(|ev| matches!(ev, HostEvent::RaiderJoined { raid: 3, id: i, wave: 4, loaded: false, .. } if *i == id));
    }
    assert!(joined, "joinRaid at the raid's wave");
    assert_eq!(h.raiders_of(3).len(), 1);
}

#[test]
fn a_vindicator_wears_the_axe_with_sharpness_by_wave() {
    have_data();
    let (mw, mut h) = world();
    let e = h.spawn(mw, "minecraft:vindicator", [0.5, 1.0, 0.5], None).expect("a known species");
    assert!(h.apply_raid_buffs(e, RaidBuff::Axe, 6, RULES));
    let axe = h.equipment_slots(e).unwrap()[0].clone().expect("an axe in the main hand");
    assert_eq!(&*axe.item, "minecraft:iron_axe");
    assert!(axe.has_any_enchantments(), "odds 1 always enchant");
}

#[test]
fn a_pillager_gets_the_crossbow_only_after_wave_three() {
    have_data();
    let (mw, mut h) = world();
    let early = h.spawn(mw, "minecraft:pillager", [0.5, 1.0, 0.5], None).expect("a known species");
    let late = h.spawn(mw, "minecraft:pillager", [4.5, 1.0, 0.5], None).expect("a known species");
    h.apply_raid_buffs(early, RaidBuff::Crossbow, 3, RULES);
    h.apply_raid_buffs(late, RaidBuff::Crossbow, 4, RULES);
    assert!(h.equipment_slots(early).unwrap()[0].is_none(), "wave 3 is not past the easy count");
    let bow = h.equipment_slots(late).unwrap()[0].clone().expect("a crossbow");
    assert_eq!(&*bow.item, "minecraft:crossbow");
    assert!(bow.has_any_enchantments());
}
