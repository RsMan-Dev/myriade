//! `propagateFallToPassengers`: a mount that landed after 7 blocks hands each rider a hit of floor(7 - 3) = 4 of cause `Fall`.
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::fall_passengers;
use horizon_ecs_proto::mount::Mount;
use horizon_ecs_proto::targeting::Who;

#[test]
fn riders_take_the_fall_of_their_mount() {
    if std::env::var_os("HORIZON_TEST_RESOURCES").is_none() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let table = species_table().0;
    let sp = table.iter().find(|s| s.name == "minecraft:zombie").expect("zombie species");
    let rider = spawn_species(&mut h.world, sp, 0.5, 0.5, 5);
    let mount = spawn_species(&mut h.world, sp, 2.5, 0.5, 6);
    h.world.entity_mut(mount).insert(Mount { rider: Some(Who::Entity(rider)), ..Default::default() });
    h.world.resource_mut::<DamageQueue>().0.clear();
    fall_passengers::propagate(&mut h.world, mount, 7.0, 1.0);
    let hits = h.world.resource::<DamageQueue>().0.clone();
    assert_eq!(hits.len(), 1);
    assert_eq!((hits[0].target, hits[0].amount), (rider, 4.0));
    h.world.resource_mut::<DamageQueue>().0.clear();
    fall_passengers::propagate(&mut h.world, mount, 2.0, 1.0);
    assert!(h.world.resource::<DamageQueue>().0.is_empty(), "a fall within 3 blocks hurts nobody");
}
