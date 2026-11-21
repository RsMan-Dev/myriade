//! `hiss`: a cat within 16 blocks of a diving flier hisses on the flier's search, the search runs once per 20 ticks, and a flier with no cat
//! near is not scared.
use horizon_ecs_proto::cow::{spawn_species, Herd};
use horizon_ecs_proto::goals::Events;
use horizon_ecs_proto::hiss;
use horizon_ecs_proto::host_out::HostEvent;

fn hisses(h: &mut Herd) -> usize {
    h.world.resource_mut::<Events>().host.drain().filter(|e| matches!(e, HostEvent::Sound { name, .. } if *name == hiss::CAT_HISS.adult)).count()
}

#[test]
fn cat_hisses_when_a_flier_searches_near_it() {
    if std::env::var_os("HORIZON_TEST_RESOURCES").is_none() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let table = h.world.resource::<horizon_ecs_proto::cow::SpeciesTable>().0.clone();
    let cat = *table.iter().find(|s| s.name == "minecraft:cat").unwrap();
    let phantom = *table.iter().find(|s| s.name == "minecraft:phantom").unwrap();
    let flier = spawn_species(&mut h.world, &phantom, 0.5, 0.5, 3);
    assert_eq!(hiss::scare(&mut h.world, flier, 100), Some(false), "no cat near: not scared");
    let _ = hisses(&mut h);
    let _c = spawn_species(&mut h.world, &cat, 5.5, 0.5, 4);
    assert_eq!(hiss::scare(&mut h.world, flier, 110), None, "the next search is not due yet");
    assert_eq!(hisses(&mut h), 0);
    assert_eq!(hiss::scare(&mut h.world, flier, 121), Some(true), "20 ticks later the cat hisses and the flier is scared");
    assert_eq!(hisses(&mut h), 1);
}
