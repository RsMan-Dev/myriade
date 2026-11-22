//! A spawn egg on its own species, a collar dye, the golden dandelion on a baby. Expected values come from the jar
//! (`SpawnEggItem.spawnOffspringFromSpawnEgg`, `Wolf.mobInteract`, `AgeableMob.mobInteract`).
use crate::common;
use bevy_ecs::prelude::{Entity, With};
use common::*;
use horizon_core::nbt::Tag;
use horizon_ecs_proto::age_lock::AgeLocked;
use horizon_ecs_proto::behaviours::Interaction;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::dye::Collar;
use horizon_ecs_proto::goals::*;
use horizon_ecs_proto::items;
use horizon_ecs_proto::tame::Owner;
use horizon_ecs_proto::targeting::Who;
use horizon_ecs_proto::used_stack::UsedStack;

fn spawn(h: &mut Herd, name: &str) -> Option<Entity> {
    let sp = species_table().0.into_iter().find(|s| s.name == name)?;
    let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 7);
    h.world.resource_mut::<Entered>().0.push(e);
    Some(e)
}

fn hold(h: &mut Herd, item: u32) {
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 0.5, y: 1.0, z: 2.5, main: item, off: 0, ..Default::default() });
}

#[test]
fn egg_on_its_own_species_makes_a_named_baby() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let Some(parent) = spawn(&mut h, "minecraft:sheep") else { return };
    let egg = items::item_id("minecraft:sheep_spawn_egg");
    hold(&mut h, egg);
    let mut stack = horizon_client_lib::items::ItemStack::new("minecraft:sheep_spawn_egg", 1);
    stack.data_mut().custom_name = Some(Tag::String("Blade".into()));
    h.world.insert_resource(UsedStack { stack, creative: false });
    assert!(interact(&mut h.world, parent, &Interaction { player: 1, item: egg }));
    let babies: Vec<Entity> = h.world.query_filtered::<Entity, With<GrowAt>>().iter(&h.world).collect();
    assert_eq!(babies.len(), 1, "one baby");
    assert_eq!(horizon_ecs_proto::custom_name::get(&h.world, babies[0]).as_deref(), Some("Blade"));
    let other = items::item_id("minecraft:cow_spawn_egg");
    hold(&mut h, other);
    assert!(!interact(&mut h.world, parent, &Interaction { player: 1, item: other }), "another species' egg does nothing here");
}

#[test]
fn collar_dye_of_the_same_colour_is_refused() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let Some(wolf) = spawn(&mut h, "minecraft:wolf") else { return };
    h.world.entity_mut(wolf).insert(Owner(Some(Who::Player(1))));
    let blue = items::item_id("minecraft:blue_dye");
    hold(&mut h, blue);
    assert!(interact(&mut h.world, wolf, &Interaction { player: 1, item: blue }));
    assert_eq!(h.world.get::<Collar>(wolf).map(|c| c.0), Some(11), "blue is ordinal 11");
    // The same colour is not the dye's: the click falls to the owner's own (sit), and the collar stays.
    interact(&mut h.world, wolf, &Interaction { player: 1, item: blue });
    assert_eq!(h.world.get::<Collar>(wolf).map(|c| c.0), Some(11));
}

#[test]
fn golden_dandelion_locks_and_unlocks_a_baby() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let Some(cow) = spawn(&mut h, "minecraft:cow") else { return };
    let dandelion = items::item_id("minecraft:golden_dandelion");
    hold(&mut h, dandelion);
    let click = |h: &mut Herd| interact(&mut h.world, cow, &Interaction { player: 1, item: dandelion });
    assert!(!click(&mut h), "an adult cannot be locked");
    h.world.entity_mut(cow).insert(GrowAt(10_000));
    assert!(click(&mut h));
    assert!(h.world.get::<AgeLocked>(cow).is_some());
    for _ in 0..50 {
        h.tick();
    }
    assert!(h.world.get::<AgeLocked>(cow).is_some(), "locked, it stays a baby");
    assert!(click(&mut h));
    assert!(h.world.get::<AgeLocked>(cow).is_none(), "a second flip unlocks");
}

#[test]
fn wolf_armor_is_placed_with_its_damage() {
    use horizon_client_lib::mobs::world::leaked_test_world_mut;
    use horizon_ecs_proto::bridge::host::{EcsWorld, HostSetup, PlayerFact};
    have_data();
    let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&HostSetup { seed: 7, dimension: "overworld", simulation_radius: 4 }, mw, types, &tags);
    let wolf = h.spawn(mw, "minecraft:wolf", [0.5, 1.0, 0.5], None).expect("a wolf");
    h.world_mut().entity_mut(wolf).insert(Owner(Some(Who::Player(1))));
    let armor = items::item_id("minecraft:wolf_armor");
    h.set_player(Player { id: 1, x: 0.5, y: 1.0, z: 2.5, main: armor, ..Default::default() }, PlayerFact { alive: true, targetable: true, hero: None });
    let mut stack = horizon_client_lib::items::ItemStack::new("minecraft:wolf_armor", 1);
    stack.damage = 10;
    assert!(h.interact_with(mw, wolf, 1, 0, stack, false));
    assert_eq!(horizon_ecs_proto::equip_stack::get(h.world(), wolf, 6).map(|s| s.damage), Some(10), "the armor keeps its wear");
}
