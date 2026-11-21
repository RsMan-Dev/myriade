//! A stab that passes the `dismount` condition makes the target get off its mount; a target that rides nothing is not affected.
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::mount::{self, Mount, Riding};
use horizon_ecs_proto::stab_dismount::stop_riding;
use horizon_ecs_proto::targeting::Who;

#[test]
fn the_stabbed_rider_gets_off() {
    if std::env::var_os("HORIZON_TEST_RESOURCES").is_none() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let table = species_table().0;
    let sp = table.iter().find(|s| s.name == "minecraft:zombie").expect("zombie species");
    let rider = spawn_species(&mut h.world, sp, 0.5, 0.5, 5);
    let mount = spawn_species(&mut h.world, sp, 2.5, 0.5, 6);
    h.world.entity_mut(mount).insert(Mount::default());
    assert!(mount::ride(&mut h.world, mount, Who::Entity(rider)));
    assert!(h.world.get::<Riding>(rider).is_some());
    assert!(stop_riding(&mut h.world, Who::Entity(rider)));
    assert!(h.world.get::<Riding>(rider).is_none());
    assert!(h.world.get::<Mount>(mount).unwrap().rider.is_none());
    assert!(!stop_riding(&mut h.world, Who::Entity(rider)), "it rides nothing now");
}
