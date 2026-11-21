//! `gift_drop`: a gift table falls from the entity's place and its sound goes out only when something fell.
use horizon_ecs_proto::cow::{spawn_species, Herd, SpeciesTable};
use horizon_ecs_proto::gift_drop::{drop_gift, GiftDrop, GiftSound};
use horizon_ecs_proto::goals::Events;
use horizon_ecs_proto::host_out::HostEvent;

static LAY: GiftDrop = GiftDrop {
    table: "minecraft:gameplay/chicken_lay",
    sound: Some(GiftSound { name: "minecraft:entity.chicken.egg", volume: 1.0 }),
    game_event: None,
    needs_mob_drops: false,
};

#[test]
fn the_egg_table_drops_and_plays_its_sound() {
    if std::env::var_os("HORIZON_TEST_RESOURCES").is_none() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let hen = *table.iter().find(|s| s.name == "minecraft:chicken").unwrap();
    let e = spawn_species(&mut h.world, &hen, 0.5, 0.5, 61);
    h.world.resource_mut::<Events>().host.drain().for_each(drop);
    assert!(drop_gift(&mut h.world, e, &LAY));
    let sounded = h.world.resource_mut::<Events>().host.drain().any(|ev| matches!(ev, HostEvent::Sound { name, .. } if name == "minecraft:entity.chicken.egg"));
    assert!(sounded);
}
