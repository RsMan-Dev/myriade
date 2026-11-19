//! What a killed animal leaves on the ground, the pickup of an item by a creature and the drop of what it holds, against the originals.
//! `HORIZON_TEST_RESOURCES` set; the species of the process (`HORIZON_SPECIES`, a cow by default) has a loot table.
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_client_lib::items::ItemStack;
use horizon_client_lib::mobs::drops::{die, Death, Drop};
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::goals::*;
use horizon_ecs_proto::item::*;
use horizon_ecs_proto::loot::DeathSeed;
use horizon_world::loot::{LootData, PackOverlay};

fn loot() -> LootData {
    let root = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").unwrap());
    let defaults = horizon_world::item_defaults::read(&root).unwrap();
    LootData::load(&root, &PackOverlay::default(), &defaults).unwrap()
}
fn sorted(mut v: Vec<(String, i32)>) -> Vec<(String, i32)> {
    v.sort();
    v
}

/// The cow killed with a table seed drops what the `Mob` drops with the same seed: the same items, the same counts, on fire (cooked) or not.
#[test]
fn a_killed_cow_leaves_the_loot_of_the_original_with_the_same_seed() {
    if !have_data() || std::env::var("HORIZON_SPECIES").as_deref() != Ok("minecraft:cow") {
        return;
    }
    let data = loot();
    let mut r = MobRef::new(16, false);
    let mut h = Herd::with_slab(16, true, false);
    let sp = *species();
    let mut shown = 0;
    for (k, fire) in (0..60).map(|k| (k, k % 3 == 0)) {
        let seed = 1000 + 7919 * k as i64;
        let i = r.add(Vec3::new(0.5, 1.0, 0.5));
        r.mobs[i].extra.death_table_seed = seed;
        if fire {
            r.mobs[i].fire_ticks = 100;
        }
        let d = Death { loot: &data, damage: None, player: None, player_killed: false, luck: 0.0, vehicle: None, mob_drops: true, world_seed: 0 };
        let (drops, _) = die(&mut r.mobs[i], &d, &mut Default::default(), &mut horizon_client_lib::mobs::world::level_random(7));
        let want = sorted(drops.into_iter().filter_map(|d| if let Drop::Item(s) = d { Some((s.item, s.count)) } else { None }).collect());
        let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 9 + k as u64);
        h.world.entity_mut(e).insert(DeathSeed(seed));
        if fire {
            h.world.entity_mut(e).insert(Burning { until: 10_000 });
        }
        h.hit(e, 1000.0, (0.0, 0.0));
        let before = h.world.resource::<ItemIndex>().len();
        h.tick();
        let got = sorted(
            h.world
                .query::<&Item>()
                .iter(&h.world)
                .map(|i| (i.stack.item.to_string(), i.stack.count as i32))
                .collect(),
        );
        // The items of the earlier rounds are on the ground too: take them off.
        let mut ids = Vec::new();
        for (e2, _) in h.world.query::<(Entity, &Item)>().iter(&h.world) {
            ids.push(e2);
        }
        for e2 in ids {
            h.world.entity_mut(e2).despawn();
        }
        let _ = before;
        if shown < 3 {
            eprintln!("seed {seed} fire {fire}: original {want:?}, ecs {got:?}");
            shown += 1;
        }
        assert_eq!(got, want, "seed {seed}, on fire {fire}");
    }
}

/// An item cannot be picked up before its delay is over; a creature takes one of the stack into its hand, the rest stays, and the event is raised.
#[test]
fn a_creature_picks_up_one_of_a_stack_after_the_delay() {
    if horizon_ecs_proto::cow::skip_unless_walker("a_creature_picks_up_one_of_a_stack_after_the_delay") {
        return;
    }
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab(16, true, false);
    let creature = spawn_species(&mut h.world, species(), 0.5, 4.5, 3);
    let item = drop_item(&mut h.world, ItemStack::new("minecraft:sweet_berries", 5), Vec3::new(0.5, 1.0, 0.5), Vec3::ZERO, 10);
    for _ in 0..9 {
        h.tick();
    }
    assert!(pick_up(&mut h.world, creature, item, 1).is_none(), "taken before its delay is over");
    h.tick();
    h.tick();
    let taken = pick_up(&mut h.world, creature, item, 1).expect("it can be taken now");
    set_main_hand(&mut h.world, creature, Some(taken));
    assert_eq!(h.world.get::<Item>(item).unwrap().stack.count, 4);
    assert_eq!(&*main_hand(&h.world, creature).unwrap().item, "minecraft:sweet_berries");
    h.tick();
    assert!(h.world.resource::<Events>().picked_up.iter().any(|c| c == creature), "the picking is not an event");
}

/// What a creature holds goes out onto the ground when it drops it, and comes out first when it dies.
#[test]
fn what_a_creature_holds_is_dropped_and_is_left_at_its_death() {
    if horizon_ecs_proto::cow::skip_unless_walker("what_a_creature_holds_is_dropped_and_is_left_at_its_death") {
        return;
    }
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab(16, true, false);
    let creature = spawn_species(&mut h.world, species(), 0.5, 4.5, 3);
    set_main_hand(&mut h.world, creature, Some(ItemStack::new("minecraft:sweet_berries", 1)));
    let mut r = horizon_core::random::LegacyRandom::new(5);
    let dropped = drop_main_hand(&mut h.world, creature, &mut r).expect("something was held");
    assert!(main_hand(&h.world, creature).is_none());
    assert_eq!(&*h.world.get::<Item>(dropped).unwrap().stack.item, "minecraft:sweet_berries");
    set_main_hand(&mut h.world, creature, Some(ItemStack::new("minecraft:glow_berries", 1)));
    h.hit(creature, 1000.0, (0.0, 0.0));
    h.tick();
    let held: Vec<_> = h.world.query::<&Item>().iter(&h.world).map(|i| i.stack.item.to_string()).collect();
    assert!(held.iter().any(|n| n == "minecraft:glow_berries"), "the held item was not left: {held:?}");
}
