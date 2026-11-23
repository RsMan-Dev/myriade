//! Death drops with the `mob_drops` rule off (`LivingEntity.die`): a piglin's inventory is mob loot and stays with it, a horse's chest is
//! equipment and still empties. Expected values are the old Mob's `drops_tests.rs` and vanilla 26.3 (`Piglin.dropCustomDeathLoot`), and
//! a tropical fish follows the first fish of its school (`TropicalFish.finalizeSpawn`).
use crate::common::*;
use bevy_ecs::prelude::*;
use myriade_engine_lib::items::ItemStack;
use myriade_engine_lib::mobs::MobWorld;
use horizon_ecs_proto::behaviours::follow_flock_leader::School;
use horizon_ecs_proto::bridge::host::PlayerFact;
use horizon_ecs_proto::bridge::players::set_player;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::credit::PlayerCredit;
use horizon_ecs_proto::goals::Player;
use horizon_ecs_proto::host_out::HostEvent;
use horizon_ecs_proto::item::Item;
use horizon_ecs_proto::item_store::ItemStore;
use horizon_ecs_proto::loot::{note_killer, DeathSeed};
use horizon_ecs_proto::mount::Mount;
use horizon_ecs_proto::mount_inventory;
use horizon_ecs_proto::spawn_env::SpawnEnv;
use horizon_ecs_proto::spawning::Spawner;

/// The `mob_drops` rule off for this world: the level's mob world copied with it cleared, as `loot_tueur.rs` does.
fn mob_drops_off(h: &mut Herd) {
    let mut level = *h.world.resource::<Level>();
    let off: &'static MobWorld<'static> = Box::leak(Box::new(MobWorld { mob_drops: false, ..level.mw.0.clone() }));
    level.mw = Mw(off);
    *h.world.resource_mut::<Level>() = level;
}

/// Player 1 kills `e` with one blow in the next tick, so the death is credited and its loot rolled.
fn kill(h: &mut Herd, e: Entity) {
    let player = Player { id: 1, x: 0.5, y: 1.0, z: 0.5, main: 0, off: 0, head: 0, chest: 0, legs: 0, feet: 0 };
    set_player(&mut h.world, player, PlayerFact { alive: true, targetable: true, hero: None, ..Default::default() });
    h.world.entity_mut(e).insert((DeathSeed(77), PlayerCredit { player: Some(1), uuid: None, until: u64::MAX }));
    note_killer(&mut h.world, e, 1, 0.0, None);
    h.world.resource_mut::<DamageQueue>().add_by(e, 1000.0, (0.0, 0.0), 1);
    h.tick();
}

/// How many of the item `id` lie on the ground.
fn dropped_count(h: &mut Herd, id: &str) -> i32 {
    h.world.query::<&Item>().iter(&h.world).filter(|i| &*i.stack.item == id).map(|i| i.stack.count as i32).sum()
}

/// `Piglin.dropCustomDeathLoot` is inside `shouldDropLoot`: with `doMobLoot` off the inventory stays with the piglin.
#[test]
fn a_piglins_inventory_stays_when_mob_loot_is_off() {
    if !have_data() || wrong_species(&["minecraft:piglin"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(16, true, false);
    mob_drops_off(&mut h);
    let sp = *species();
    let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 9);
    h.world.get_mut::<ItemStore>(e).expect("an inventory").items[0] = Some(ItemStack::new("minecraft:gold_ingot", 3));
    kill(&mut h, e);
    assert_eq!(dropped_count(&mut h, "minecraft:gold_ingot"), 0, "the inventory stays with the piglin");
}

/// `AbstractHorse.dropEquipment` is outside `shouldDropLoot`: with `doMobLoot` off a donkey's chest still drops its stacks.
#[test]
fn a_donkey_chest_drops_with_mob_loot_off() {
    if !have_data() || wrong_species(&["minecraft:donkey"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(16, true, false);
    mob_drops_off(&mut h);
    let sp = *species();
    let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 9);
    h.world.entity_mut(e).insert(Mount { max_temper: 1, ..Default::default() });
    mount_inventory::chest_on(&mut h.world, e, 5);
    assert!(mount_inventory::set_items(&mut h.world, e, vec![Some(ItemStack::new("minecraft:diamond", 4))]), "a chest to fill");
    kill(&mut h, e);
    assert_eq!(dropped_count(&mut h, "minecraft:diamond"), 4, "the chest's stack drops");
}

/// `TropicalFish.finalizeSpawn`: a school's first fish leads, and each later fish of its group follows that leader (`startFollowing`),
/// which counts it (`addFollower`): the leader's followers are the group's size less one.
#[test]
fn a_school_fish_follows_the_first_fish_of_its_group() {
    if !have_data() || wrong_species(&["minecraft:tropical_fish"]) {
        return;
    }
    let mut h = Herd::with_spawn_world_awake(176, true);
    h.enable_spawning("minecraft:plains", 7);
    h.fill_water(4);
    h.set_online(&[(8.0, 8.0)]);
    h.world.insert_resource(SpawnEnv { spawn_mobs: true, monsters: false, spawnable: (-10..=10).flat_map(|dx| (-10..=10).map(move |dz| (dx, dz))).collect(), border: None, world_spawn: [-200, 1, -200], changed: false });
    h.world.resource_mut::<Spawner>().groups = Some(Vec::new());
    // Each birth with its leader, and that leader's followers, as they stand at the end of the tick of the birth.
    let mut born: Vec<(Entity, Option<Entity>, i32)> = Vec::new();
    for _ in 0..800 {
        h.tick();
        for ev in h.drain_host_out() {
            if let HostEvent::Spawned { entity, natural: true, .. } = ev {
                let leader = h.world.get::<School>(entity).and_then(|s| s.leader);
                let followers = leader.and_then(|l| h.world.get::<School>(l)).map_or(0, |s| s.followers);
                born.push((entity, leader, followers));
            }
        }
    }
    let sizes = h.world.resource_mut::<Spawner>().groups.take().unwrap_or_default();
    let mut rest = born.into_iter();
    let mut schools = 0u32;
    for &n in &sizes {
        let group: Vec<(Entity, Option<Entity>, i32)> = rest.by_ref().take(n as usize).collect();
        let Some(&(first, _, _)) = group.first() else { continue };
        let name = species_table().0[usize::from(h.world.get::<SpeciesId>(first).unwrap().0)].name;
        if name != "minecraft:tropical_fish" || group.len() < 2 {
            continue;
        }
        schools += 1;
        for &(fish, leader, _) in &group[1..] {
            assert_eq!(leader, Some(first), "a follower of the first fish of its school: {fish:?}");
        }
        let &(_, _, followers) = group.last().unwrap();
        assert_eq!(followers, group.len() as i32 - 1, "the leader counts its followers");
    }
    assert!(schools > 0, "{schools} schools of tropical fish born");
}
