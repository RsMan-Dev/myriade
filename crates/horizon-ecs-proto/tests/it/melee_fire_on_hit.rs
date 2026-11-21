//! Point 1: a burning attacker with an empty hand ignites what it hits (`Zombie.doHurtTarget`); a calm one does not.
use horizon_ecs_proto::cow::{spawn_species, Burning, Herd, SpeciesTable};
use horizon_ecs_proto::hostile::{landed, IgniteOnHit, ZOMBIE_FIRE};
use horizon_ecs_proto::targeting::Who;

#[test]
fn a_burning_attacker_ignites_its_target() {
    if std::env::var_os("HORIZON_TEST_RESOURCES").is_none() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let cow = *h.world.resource::<SpeciesTable>().0.iter().find(|s| s.name == "minecraft:cow").unwrap();
    let attacker = spawn_species(&mut h.world, &cow, 0.5, 0.5, 61);
    let prey = spawn_species(&mut h.world, &cow, 2.5, 0.5, 62);
    h.world.entity_mut(attacker).insert(IgniteOnHit(&ZOMBIE_FIRE));
    landed(&mut h.world, attacker, Who::Entity(prey), 3.0);
    assert!(h.world.get::<Burning>(prey).is_none(), "an attacker that does not burn sets nothing on fire");
    horizon_ecs_proto::cow::set_on_fire(&mut h.world, attacker, 200);
    // The local difficulty gives a chance per blow (0.3 per point): a few hundred blows always set it alight.
    for _ in 0..300 {
        landed(&mut h.world, attacker, Who::Entity(prey), 3.0);
    }
    assert!(h.world.get::<Burning>(prey).is_some());
}
