//! The right-click rules written by action: coating (wax, axe), hand over / take back, piglin gift, llama carpet and chest, harness, nautilus taming
//! use up the item. Expected values come from the jar (`CopperGolem.mobInteract`, `Allay.mobInteract`, `AbstractChestedHorse.mobInteract`).
use crate::common;
use bevy_ecs::entity::Entity;
use common::*;
use horizon_ecs_proto::behaviours::Interaction;
use horizon_ecs_proto::coating::Coat;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::goals::*;
use horizon_ecs_proto::items;
use horizon_ecs_proto::tame::Owner;
use horizon_ecs_proto::targeting::Who;

fn species_named(name: &str) -> Option<Species> {
    species_table().0.into_iter().find(|s| s.name == name)
}
fn hold(h: &mut Herd, main: u32) {
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 0.5, y: 1.0, z: 2.5, main, off: 0, ..Default::default() });
}
fn click(h: &mut Herd, e: Entity, item: u32) -> bool {
    interact(&mut h.world, e, &Interaction { player: 1, item })
}
fn main_of(h: &Herd) -> u32 {
    h.world.resource::<Online>().get(1).unwrap().main
}
fn spawn(h: &mut Herd, name: &str) -> Option<Entity> {
    let sp = species_named(name)?;
    let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 7);
    h.world.resource_mut::<Entered>().0.push(e);
    Some(e)
}

#[test]
fn honeycomb_waxes_the_golem_and_an_axe_scrapes_the_wax_off() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let Some(e) = spawn(&mut h, "minecraft:copper_golem") else { return };
    let comb = items::item_id("minecraft:honeycomb");
    hold(&mut h, comb);
    assert!(click(&mut h, e, comb));
    assert!(h.world.get::<Coat>(e).unwrap().waxed);
    assert_eq!(main_of(&h), 0, "the honeycomb is used up");
    assert!(!click(&mut h, e, comb), "a waxed golem refuses more wax");
    let axe = items::item_id("minecraft:iron_axe");
    hold(&mut h, axe);
    assert!(click(&mut h, e, axe));
    assert!(!h.world.get::<Coat>(e).unwrap().waxed);
    assert!(!click(&mut h, e, axe), "a plain golem has nothing to scrape");
}

#[test]
fn an_axe_steps_the_oxidation_back_by_one() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let Some(e) = spawn(&mut h, "minecraft:copper_golem") else { return };
    h.world.entity_mut(e).insert(Coat { stage: 2, ..Default::default() });
    let axe = items::item_id("minecraft:iron_axe");
    hold(&mut h, axe);
    assert!(click(&mut h, e, axe));
    assert_eq!(h.world.get::<Coat>(e).unwrap().stage, 1);
}

#[test]
fn an_allay_takes_an_item_and_gives_it_back_to_an_empty_hand() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let Some(e) = spawn(&mut h, "minecraft:allay") else { return };
    let cod = items::item_id("minecraft:cod");
    hold(&mut h, cod);
    assert!(click(&mut h, e, cod));
    assert_eq!(main_of(&h), 0, "the item left the player's hand");
    assert_eq!(h.world.get::<Owner>(e).unwrap().0, Some(Who::Player(1)), "the giver is the liked player");
    assert!(click(&mut h, e, 0), "an empty hand takes it back");
    assert!(h.world.get::<horizon_ecs_proto::pocket::Pocket>(e).unwrap().held().is_none());
}

#[test]
fn a_piglin_takes_a_gold_ingot_in_its_off_hand() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let Some(e) = spawn(&mut h, "minecraft:piglin") else { return };
    let gold = items::item_id("minecraft:gold_ingot");
    hold(&mut h, gold);
    assert!(click(&mut h, e, gold));
    assert_eq!(equipped(&h.world, e, Slot::Off), gold);
    assert_eq!(main_of(&h), 0);
}

#[test]
fn a_llama_wears_a_carpet_untamed_and_takes_a_chest_only_tame() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let Some(e) = spawn(&mut h, "minecraft:llama") else { return };
    let carpet = items::item_id("minecraft:white_carpet");
    hold(&mut h, carpet);
    assert!(click(&mut h, e, carpet));
    assert_eq!(equipped(&h.world, e, Slot::Body), carpet);
    let chest = items::item_id("minecraft:chest");
    hold(&mut h, chest);
    // The click is not lost (a wild llama is mounted or angered), but the chest does not go on.
    click(&mut h, e, chest);
    assert!(h.world.get::<horizon_ecs_proto::mount_inventory::MountInventory>(e).is_none(), "a wild llama refuses the chest");
    h.world.entity_mut(e).insert(Owner(Some(Who::Player(1))));
    assert!(click(&mut h, e, chest));
    assert!(h.world.get::<horizon_ecs_proto::mount_inventory::MountInventory>(e).is_some());
    assert_eq!(main_of(&h), 0);
}

#[test]
fn a_harness_is_worn_once_and_used_up() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let Some(e) = spawn(&mut h, "minecraft:happy_ghast") else { return };
    let harness = items::item_id("minecraft:white_harness");
    hold(&mut h, harness);
    assert!(click(&mut h, e, harness));
    assert_eq!(main_of(&h), 0);
    assert!(!click(&mut h, e, harness), "one harness only");
}

#[test]
fn a_nautilus_taming_item_is_used_up_even_when_the_taming_fails() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let Some(e) = spawn(&mut h, "minecraft:nautilus") else { return };
    let fish = items::item_id("minecraft:pufferfish");
    hold(&mut h, fish);
    assert!(click(&mut h, e, fish));
    assert_eq!(main_of(&h), 0);
}
