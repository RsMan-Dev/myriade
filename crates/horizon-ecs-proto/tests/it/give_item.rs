//! `give_item::GIVE`: an iron ingot heals a hurt iron golem by 25 (refused at full health), a golden apple under Weakness starts the
//! zombie villager's cure (Weakness gone, Strength on, deferred conversion into a villager), and without Weakness the apple stays in the hand.
use crate::common;
use common::*;
use horizon_ecs_proto::behaviours::Interaction;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::effects::{self, Kind};
use horizon_ecs_proto::goals::*;
use horizon_ecs_proto::items;

fn species_named(name: &str) -> Option<Species> {
    species_table().0.into_iter().find(|s| s.name == name)
}

#[test]
fn an_iron_ingot_heals_a_hurt_golem_by_25() {
    if !have_data() {
        return;
    }
    let Some(sp) = species_named("minecraft:iron_golem") else { return };
    let mut h = Herd::with_slab_awake(60, true, false);
    let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 7);
    h.world.resource_mut::<Entered>().0.push(e);
    let ingot = items::item_id("minecraft:iron_ingot");
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 0.5, y: 1.0, z: 2.5, main: ingot, off: 0, ..Default::default() });
    let max = h.world.get::<MaxHealth>(e).unwrap().0;
    assert!(!interact(&mut h.world, e, &Interaction { player: 1, item: ingot }), "a golem at full health refuses the ingot");
    h.world.get_mut::<Health>(e).unwrap().0 = max - 40.0;
    assert!(interact(&mut h.world, e, &Interaction { player: 1, item: ingot }));
    assert_eq!(h.world.get::<Health>(e).unwrap().0, max - 15.0);
    assert_eq!(h.world.resource::<Online>().get(1).unwrap().main, 0, "the ingot left the hand");
}

#[test]
fn a_golden_apple_under_weakness_cures_the_zombie_villager_later() {
    if !have_data() {
        return;
    }
    let Some(sp) = species_named("minecraft:zombie_villager") else { return };
    let mut h = Herd::with_slab_awake(60, true, false);
    let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 8);
    h.world.resource_mut::<Entered>().0.push(e);
    let apple = items::item_id("minecraft:golden_apple");
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 0.5, y: 1.0, z: 2.5, main: apple, off: 0, ..Default::default() });
    assert!(interact(&mut h.world, e, &Interaction { player: 1, item: apple }), "the click is handled without Weakness");
    assert_eq!(h.world.resource::<Online>().get(1).unwrap().main, apple, "but the apple is kept");
    effects::add(&mut h.world, e, Kind::Weakness, 0, 6000);
    assert!(interact(&mut h.world, e, &Interaction { player: 1, item: apple }));
    assert_eq!(h.world.resource::<Online>().get(1).unwrap().main, 0, "the apple was eaten");
    assert!(!effects::has(&h.world, e, Kind::Weakness));
    assert!(effects::has(&h.world, e, Kind::Strength));
    for _ in 0..6001 {
        h.tick();
    }
    assert!(h.world.get_entity(e).is_err(), "the zombie villager is gone after the cure");
    let villager = h.world.query::<&SpeciesId>().iter(&h.world).any(|s| species_table_name(*s) == "minecraft:villager");
    assert!(villager, "a villager stands where it was");
}

fn species_table_name(id: SpeciesId) -> &'static str {
    species_table().0[id.0 as usize].name
}
