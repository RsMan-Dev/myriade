//! `Villager.tellWitnessesThatIWasMurdered`: a villager killed by a player is witnessed by the villagers that see it within 16 blocks; each
//! takes the killing as a major negative gossip of 25 about that player (`VILLAGER_KILLED`, `Villager.onReputationEventFrom`).
use horizon_ecs_proto::behaviours::gossip::{witness_murder, Gossips, Kind};
use horizon_ecs_proto::cow::{spawn_species, Herd, SpeciesTable};

#[test]
fn a_villager_that_sees_a_murder_holds_a_major_negative_gossip_of_25() {
    if std::env::var_os("HORIZON_TEST_RESOURCES").is_none() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let villager = *table.iter().find(|s| s.name == "minecraft:villager").unwrap();
    let dead = spawn_species(&mut h.world, &villager, 0.5, 0.5, 5);
    let witness = spawn_species(&mut h.world, &villager, 3.5, 0.5, 5);
    witness_murder(&mut h.world, dead, 1);
    let g = h.world.get::<Gossips>(witness).expect("the witness holds gossip");
    assert_eq!(g.get(1, Kind::MajorNegative), -25);
    assert!(h.world.get::<Gossips>(dead).is_none(), "the victim witnesses nothing");
}

#[test]
fn an_invisible_witness_three_blocks_away_is_not_seen_from_the_victim() {
    if std::env::var_os("HORIZON_TEST_RESOURCES").is_none() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let villager = *table.iter().find(|s| s.name == "minecraft:villager").unwrap();
    let dead = spawn_species(&mut h.world, &villager, 0.5, 0.5, 5);
    let witness = spawn_species(&mut h.world, &villager, 3.5, 0.5, 5);
    h.world.entity_mut(witness).insert(horizon_ecs_proto::effects::Invisible);
    witness_murder(&mut h.world, dead, 1);
    assert!(h.world.get::<Gossips>(witness).is_none(), "an invisible witness 3 blocks away is out of the sensor's reach (2 blocks)");
}
