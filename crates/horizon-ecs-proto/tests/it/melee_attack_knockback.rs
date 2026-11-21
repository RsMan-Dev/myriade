//! Point 2: `knock` is `LivingEntity.knockback`: the target's resistance reduces the push; the velocity is halved and pushed along (x, z).
use horizon_ecs_proto::cow::{spawn_species, Herd, Resist, SpeciesTable, Vel};
use horizon_ecs_proto::hostile::knock;

#[test]
fn the_resistance_scales_the_push() {
    if std::env::var_os("HORIZON_TEST_RESOURCES").is_none() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let cow = *h.world.resource::<SpeciesTable>().0.iter().find(|s| s.name == "minecraft:cow").unwrap();
    let t = spawn_species(&mut h.world, &cow, 2.5, 0.5, 62);
    h.world.entity_mut(t).insert(Resist(0.5));
    h.world.get_mut::<Vel>(t).unwrap().0.x = 0.0;
    h.world.get_mut::<Vel>(t).unwrap().0.z = 0.0;
    knock(&mut h.world, t, 0.8, 0.0, -1.0);
    let v = h.world.get::<Vel>(t).unwrap().0;
    assert!((v.z - 0.4).abs() < 1e-9, "{}", v.z);
    assert!(v.x.abs() < 1e-9);
}
