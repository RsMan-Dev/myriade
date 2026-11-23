//! `Entity.invulnerable` (the `Invulnerable` flag): an invulnerable creature is not a target (`TargetingConditions`, the warden's
//! `canTargetEntity`); absent, the creature stays a target.
use crate::common;
use common::*;
use horizon_ecs_proto::cow::{spawn_species, Herd, SpeciesTable};
use horizon_ecs_proto::invulnerable::Invulnerable;
use horizon_ecs_proto::targeting::{can_target, Who};

#[test]
fn an_invulnerable_creature_is_not_a_target() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let zombie = *table.iter().find(|s| s.name == "minecraft:zombie").unwrap();
    let watcher = spawn_species(&mut h.world, &zombie, 0.5, 0.5, 5);
    let target = spawn_species(&mut h.world, &zombie, 1.5, 0.5, 5);
    assert!(can_target(&h.world, watcher, Who::Entity(target)), "a creature without the flag is a target");
    h.world.entity_mut(target).insert(Invulnerable);
    assert!(!can_target(&h.world, watcher, Who::Entity(target)), "an invulnerable creature is not a target");
}
